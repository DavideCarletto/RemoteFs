use std::collections::HashMap;
use std::time::{Duration, Instant};
use log::{debug, info, warn};
use crate::filesystem::FileMetadata;

#[derive(Clone)]
pub struct CachedMetadata {
    pub metadata: FileMetadata,
    pub cached_at: Instant,
    pub ttl: Duration,
}

impl CachedMetadata {
    pub fn new(metadata: FileMetadata, ttl: Duration) -> Self {
        Self {
            metadata,
            cached_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
}

#[derive(Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub ino: u64,
    pub file_type: fuser::FileType,
}

#[derive(Clone)]
pub struct CachedDirectory {
    pub entries: Vec<DirectoryEntry>,
    pub cached_at: Instant,
    pub ttl: Duration,
}

impl CachedDirectory {
    pub fn new(entries: Vec<DirectoryEntry>, ttl: Duration) -> Self {
        Self {
            entries,
            cached_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
}

#[derive(Clone)]
pub struct CachedFileContent {
    pub data: Vec<u8>,
    pub cached_at: Instant,
    pub ttl: Duration,
    pub file_size: u64,
}

impl CachedFileContent {
    pub fn new(data: Vec<u8>, file_size: u64, ttl: Duration) -> Self {
        Self {
            data,
            file_size,
            cached_at: Instant::now(),
            ttl,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < self.ttl
    }
}

pub struct CacheConfig {
    pub metadata_ttl: Duration,
    pub directory_ttl: Duration,
    pub inode_path_ttl: Duration,
    pub file_content_ttl: Duration,
    pub max_file_cache_size: usize,  // Byte massimi per cache file
    pub max_cached_files: usize,     // Numero massimo file in cache
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: Duration::from_secs(30),      // 30s per metadati
            directory_ttl: Duration::from_secs(60),     // 1min per directory
            inode_path_ttl: Duration::from_secs(300),   // 5min per mapping inode<->path
            file_content_ttl: Duration::from_secs(120), // 2min per contenuto file
            max_file_cache_size: 1024 * 1024,          // 1MB massimo per file
            max_cached_files: 100,                      // Massimo 100 file
        }
    }
}

pub enum CacheInvalidationStrategy {
    TTL,    // Time-to-Live
    LRU,    // Least Recently Used
    Hybrid, // TTL + LRU combination
}

pub struct FileSystemCache {
    // Cache metadati
    metadata_by_path: HashMap<String, CachedMetadata>,
    metadata_by_inode: HashMap<u64, CachedMetadata>,
    
    // Cache mappature inode <-> path
    inode_to_path: HashMap<u64, (String, Instant)>,
    path_to_inode: HashMap<String, (u64, Instant)>,
    
    // Cache directory
    directories: HashMap<String, CachedDirectory>,
    
    // Cache contenuto file (per file piccoli)
    file_contents: HashMap<String, CachedFileContent>,
    
    // Configurazione
    config: CacheConfig,
    strategy: CacheInvalidationStrategy,
    
    // Statistiche
    pub stats: CacheStats,
}

#[derive(Default, Debug)]
pub struct CacheStats {
    pub metadata_hits: u64,
    pub metadata_misses: u64,
    pub directory_hits: u64,
    pub directory_misses: u64,
    pub inode_path_hits: u64,
    pub inode_path_misses: u64,
    pub file_content_hits: u64,
    pub file_content_misses: u64,
    pub invalidations: u64,
}

impl FileSystemCache {
    pub fn new(config: CacheConfig, strategy: CacheInvalidationStrategy) -> Self {
        Self {
            metadata_by_path: HashMap::new(),
            metadata_by_inode: HashMap::new(),
            inode_to_path: HashMap::new(),
            path_to_inode: HashMap::new(),
            directories: HashMap::new(),
            file_contents: HashMap::new(),
            config,
            strategy,
            stats: Default::default(),
        }
    }

    pub fn with_default() -> Self {
        Self::new(CacheConfig::default(), CacheInvalidationStrategy::Hybrid)
    }

    // ==================== METADATA CACHE ====================
    
    pub fn get_metadata_by_path(&mut self, path: &str) -> Option<FileMetadata> {
        if let Some(cached) = self.metadata_by_path.get(path) {
            if cached.is_valid() {
                self.stats.metadata_hits += 1;
                debug!("Cache HIT: metadata per path {}", path);
                return Some(cached.metadata.clone());
            } else {
                // TTL scaduto, rimuovi
                debug!("Cache EXPIRED: metadata per path {}", path);
                self.metadata_by_path.remove(path);
            }
        }
        
        self.stats.metadata_misses += 1;
        debug!("Cache MISS: metadata per path {}", path);
        None
    }

    pub fn get_metadata_by_inode(&mut self, ino: u64) -> Option<FileMetadata> {
        if let Some(cached) = self.metadata_by_inode.get(&ino) {
            if cached.is_valid() {
                self.stats.metadata_hits += 1;
                debug!("Cache HIT: metadata per inode {}", ino);
                return Some(cached.metadata.clone());
            } else {
                debug!("Cache EXPIRED: metadata per inode {}", ino);
                self.metadata_by_inode.remove(&ino);
            }
        }
        
        self.stats.metadata_misses += 1;
        debug!("Cache MISS: metadata per inode {}", ino);
        None
    }

    pub fn cache_metadata(&mut self, path: String, metadata: FileMetadata) {
        let cached = CachedMetadata::new(metadata.clone(), self.config.metadata_ttl);
        
        debug!("Cache STORE: metadata per {} (inode {})", path, metadata.ino);
        self.metadata_by_path.insert(path.clone(), cached.clone());
        self.metadata_by_inode.insert(metadata.ino, cached);
        
        // Aggiorna anche la cache inode<->path
        self.cache_inode_path_mapping(metadata.ino, path);
    }

    // ==================== INODE <-> PATH MAPPING ====================
    
    pub fn get_path_by_inode(&mut self, ino: u64) -> Option<String> {
        if let Some((path, cached_at)) = self.inode_to_path.get(&ino) {
            if cached_at.elapsed() < self.config.inode_path_ttl {
                self.stats.inode_path_hits += 1;
                debug!("Cache HIT: path per inode {} -> {}", ino, path);
                return Some(path.clone());
            } else {
                debug!("Cache EXPIRED: path per inode {}", ino);
                self.inode_to_path.remove(&ino);
            }
        }
        
        self.stats.inode_path_misses += 1;
        debug!("Cache MISS: path per inode {}", ino);
        None
    }

    pub fn get_inode_by_path(&mut self, path: &str) -> Option<u64> {
        if let Some((ino, cached_at)) = self.path_to_inode.get(path) {
            if cached_at.elapsed() < self.config.inode_path_ttl {
                self.stats.inode_path_hits += 1;
                debug!("Cache HIT: inode per path {} -> {}", path, ino);
                return Some(*ino);
            } else {
                debug!("Cache EXPIRED: inode per path {}", path);
                self.path_to_inode.remove(path);
            }
        }
        
        self.stats.inode_path_misses += 1;
        debug!("Cache MISS: inode per path {}", path);
        None
    }

    pub fn cache_inode_path_mapping(&mut self, ino: u64, path: String) {
        let now = Instant::now();
        debug!("Cache STORE: mapping {} <-> {}", ino, path);
        self.inode_to_path.insert(ino, (path.clone(), now));
        self.path_to_inode.insert(path, (ino, now));
    }

    // ==================== DIRECTORY CACHE ====================
    
    pub fn get_directory_entries(&mut self, path: &str) -> Option<Vec<DirectoryEntry>> {
        if let Some(cached_dir) = self.directories.get(path) {
            if cached_dir.is_valid() {
                self.stats.directory_hits += 1;
                debug!("Cache HIT: directory {} ({} entries)", path, cached_dir.entries.len());
                return Some(cached_dir.entries.clone());
            } else {
                debug!("Cache EXPIRED: directory {}", path);
                self.directories.remove(path);
            }
        }
        
        self.stats.directory_misses += 1;
        debug!("Cache MISS: directory {}", path);
        None
    }

    pub fn cache_directory_entries(&mut self, path: String, entries: Vec<(String, u64, fuser::FileType)>) {
        let cached_entries: Vec<DirectoryEntry> = entries.into_iter()
            .map(|(name, ino, file_type)| DirectoryEntry { name, ino, file_type })
            .collect();
        
        let cached_dir = CachedDirectory::new(cached_entries, self.config.directory_ttl);
        debug!("Cache STORE: directory {} ({} entries)", path, cached_dir.entries.len());
        self.directories.insert(path, cached_dir);
    }

    // ==================== FILE CONTENT CACHE ====================
    
    pub fn get_file_content(&mut self, path: &str, expected_size: u64) -> Option<Vec<u8>> {
        if let Some(cached_content) = self.file_contents.get(path) {
            if cached_content.is_valid() && cached_content.file_size == expected_size {
                self.stats.file_content_hits += 1;
                debug!("Cache HIT: contenuto file {} ({} bytes)", path, cached_content.data.len());
                return Some(cached_content.data.clone());
            } else {
                debug!("Cache EXPIRED/SIZE_MISMATCH: contenuto file {}", path);
                self.file_contents.remove(path);
            }
        }
        
        self.stats.file_content_misses += 1;
        debug!("Cache MISS: contenuto file {}", path);
        None
    }

    pub fn cache_file_content(&mut self, path: String, data: Vec<u8>, file_size: u64) -> bool {
        // Non cachare file troppo grandi
        if data.len() > self.config.max_file_cache_size {
            debug!("File {} troppo grande per cache ({} bytes)", path, data.len());
            return false;
        }
        
        // Controlla limite numero file
        if self.file_contents.len() >= self.config.max_cached_files {
            self.evict_lru_file_content();
        }
        
        let cached_content = CachedFileContent::new(data, file_size, self.config.file_content_ttl);
        debug!("Cache STORE: contenuto file {} ({} bytes)", path, cached_content.data.len());
        self.file_contents.insert(path, cached_content);
        true
    }

    // ==================== INVALIDATION ====================
    
    pub fn invalidate_path(&mut self, path: &str) {
        debug!("Cache INVALIDATE: path {}", path);
        
        // Rimuovi metadati
        self.metadata_by_path.remove(path);
        
        // Rimuovi contenuto file
        self.file_contents.remove(path);
        
        // Rimuovi da mapping path->inode
        if let Some((ino, _)) = self.path_to_inode.remove(path) {
            self.inode_to_path.remove(&ino);
            self.metadata_by_inode.remove(&ino);
        }
        
        // Invalida directory padre
        if let Some(parent_path) = path.rsplitn(2, '/').nth(1) {
            let parent = if parent_path.is_empty() { "/" } else { parent_path };
            self.directories.remove(parent);
        }
        
        self.stats.invalidations += 1;
    }

    pub fn invalidate_directory(&mut self, dir_path: &str) {
        debug!("Cache INVALIDATE: directory {}", dir_path);
        self.directories.remove(dir_path);
        self.stats.invalidations += 1;
    }

    // ==================== LRU EVICTION ====================
    
    fn evict_lru_file_content(&mut self) {
        if let Some((oldest_path, _)) = self.file_contents.iter()
            .min_by_key(|(_, content)| content.cached_at)
            .map(|(path, content)| (path.clone(), content.cached_at)) {
            debug!("Cache EVICT LRU: file content {}", oldest_path);
            self.file_contents.remove(&oldest_path);
        }
    }

    // ==================== CLEANUP ====================
    
    pub fn cleanup_expired(&mut self) {
        let initial_count = self.metadata_by_path.len() + self.directories.len() + 
                          self.file_contents.len() + self.inode_to_path.len();
        
        // Cleanup metadati scaduti
        self.metadata_by_path.retain(|_, cached| cached.is_valid());
        self.metadata_by_inode.retain(|_, cached| cached.is_valid());
        
        // Cleanup directory scadute
        self.directories.retain(|_, cached| cached.is_valid());
        
        // Cleanup contenuto file scaduto
        self.file_contents.retain(|_, cached| cached.is_valid());
        
        // Cleanup mapping inode<->path scaduti
        let ttl = self.config.inode_path_ttl;
        self.inode_to_path.retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);
        self.path_to_inode.retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);
        
        let final_count = self.metadata_by_path.len() + self.directories.len() + 
                         self.file_contents.len() + self.inode_to_path.len();
        
        if initial_count > final_count {
            info!("Cache cleanup: rimossi {} elementi scaduti", initial_count - final_count);
        }
    }

    pub fn print_stats(&self) {
        info!("=== CACHE STATISTICS ===");
        
        let metadata_total = self.stats.metadata_hits + self.stats.metadata_misses;
        let directory_total = self.stats.directory_hits + self.stats.directory_misses;
        let inode_path_total = self.stats.inode_path_hits + self.stats.inode_path_misses;
        let file_content_total = self.stats.file_content_hits + self.stats.file_content_misses;
        
        if metadata_total > 0 {
            info!("Metadata: {} hits, {} misses ({:.1}% hit rate)", 
                  self.stats.metadata_hits, self.stats.metadata_misses,
                  self.stats.metadata_hits as f64 / metadata_total as f64 * 100.0);
        } else {
            info!("Metadata: nessuna operazione registrata");
        }
        
        if directory_total > 0 {
            info!("Directory: {} hits, {} misses ({:.1}% hit rate)",
                  self.stats.directory_hits, self.stats.directory_misses,
                  self.stats.directory_hits as f64 / directory_total as f64 * 100.0);
        } else {
            info!("Directory: nessuna operazione registrata");
        }
        
        if inode_path_total > 0 {
            info!("Inode-Path: {} hits, {} misses ({:.1}% hit rate)",
                  self.stats.inode_path_hits, self.stats.inode_path_misses,
                  self.stats.inode_path_hits as f64 / inode_path_total as f64 * 100.0);
        } else {
            info!("Inode-Path: nessuna operazione registrata");
        }
        
        if file_content_total > 0 {
            info!("File Content: {} hits, {} misses ({:.1}% hit rate)",
                  self.stats.file_content_hits, self.stats.file_content_misses,
                  self.stats.file_content_hits as f64 / file_content_total as f64 * 100.0);
        } else {
            info!("File Content: nessuna operazione registrata");
        }
        
        info!("Total invalidations: {}", self.stats.invalidations);
        info!("Current cache sizes: {} metadata, {} directories, {} file contents",
              self.metadata_by_path.len(), self.directories.len(), self.file_contents.len());
    }
}