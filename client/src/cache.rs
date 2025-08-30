use crate::types::FileMetadata;
use log::{debug, info};
use std::collections::HashMap;
use std::time::{Duration, Instant};

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
    #[cfg(target_os = "linux")]
    pub file_type: fuser::FileType,
    #[cfg(target_os = "windows")]
    pub file_type: String,
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
    pub max_file_cache_size: usize, 
    pub max_cached_files: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: Duration::from_secs(30),       
            directory_ttl: Duration::from_secs(60),     
            inode_path_ttl: Duration::from_secs(300),   
            file_content_ttl: Duration::from_secs(120), 
            max_file_cache_size: 1024 * 1024,           
            max_cached_files: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheInvalidationStrategy {
    TTL(Duration),    
    LRU(usize),      
}

pub struct FileSystemCache {
    metadata_by_path: HashMap<String, CachedMetadata>,
    metadata_by_inode: HashMap<u64, CachedMetadata>,

    inode_to_path: HashMap<u64, (String, Instant)>,
    path_to_inode: HashMap<String, (u64, Instant)>,

    directories: HashMap<String, CachedDirectory>,

    file_contents: HashMap<String, CachedFileContent>,

    config: CacheConfig,
    strategy: CacheInvalidationStrategy,

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
        Self::new(CacheConfig::default(), CacheInvalidationStrategy::TTL(Duration::from_secs(60)))
    }

    /// Configura la strategia di invalidazione
    pub fn set_strategy(&mut self, strategy: CacheInvalidationStrategy) {
        debug!("Cache: changing strategy to {:?}", strategy);
        self.strategy = strategy;
        // Applica immediatamente la nuova strategia
        self.apply_cache_strategy();
    }

    /// Verifica se un elemento è valido secondo la strategia corrente
    fn is_valid_by_strategy(&self, cached_at: Instant) -> bool {
        match &self.strategy {
            CacheInvalidationStrategy::TTL(duration) => cached_at.elapsed() < *duration,
            CacheInvalidationStrategy::LRU(_) => true, 
        }
    }

    pub fn get_metadata_by_path(&mut self, path: &str) -> Option<FileMetadata> {
        if let Some(cached) = self.metadata_by_path.get(path) {
            if self.is_valid_by_strategy(cached.cached_at) {
                self.stats.metadata_hits += 1;
                debug!("Cache HIT: metadata per path {}", path);
                return Some(cached.metadata.clone());
            } else {
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
            if self.is_valid_by_strategy(cached.cached_at) {
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
        self.apply_cache_strategy();
        
        let cached = CachedMetadata::new(metadata.clone(), self.config.metadata_ttl);

        debug!(
            "Cache STORE: metadata per {} (inode {})",
            path, metadata.ino
        );
        self.metadata_by_path.insert(path.clone(), cached.clone());
        self.metadata_by_inode.insert(metadata.ino, cached);

        self.cache_inode_path_mapping(metadata.ino, path);
    }

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

    pub fn get_directory_entries(&mut self, path: &str) -> Option<Vec<DirectoryEntry>> {
        if let Some(cached_dir) = self.directories.get(path) {
            if self.is_valid_by_strategy(cached_dir.cached_at) {
                self.stats.directory_hits += 1;
                debug!(
                    "Cache HIT: directory {} ({} entries)",
                    path,
                    cached_dir.entries.len()
                );
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

    #[cfg(target_os = "linux")]
    pub fn cache_directory_entries(
        &mut self,
        path: String,
        entries: Vec<(String, u64, fuser::FileType)>,
    ) {
        self.apply_cache_strategy();
        
        let cached_entries: Vec<DirectoryEntry> = entries
            .into_iter()
            .map(|(name, ino, file_type)| DirectoryEntry {
                name,
                ino,
                file_type,
            })
            .collect();

        let cached_dir = CachedDirectory::new(cached_entries, self.config.directory_ttl);
        debug!(
            "Cache STORE: directory {} ({} entries)",
            path,
            cached_dir.entries.len()
        );
        self.directories.insert(path, cached_dir);
    }

    #[cfg(target_os = "windows")]
    pub fn cache_directory_entries(&mut self, path: String, entries: Vec<(String, u64, String)>) {
        self.apply_cache_strategy();
        
        let cached_entries: Vec<DirectoryEntry> = entries
            .into_iter()
            .map(|(name, ino, file_type)| DirectoryEntry {
                name,
                ino,
                file_type,
            })
            .collect();

        let cached_dir = CachedDirectory::new(cached_entries, self.config.directory_ttl);
        debug!(
            "Cache STORE: directory {} ({} entries)",
            path,
            cached_dir.entries.len()
        );
        self.directories.insert(path, cached_dir);
    }

    pub fn get_file_content(&mut self, path: &str, expected_size: u64) -> Option<Vec<u8>> {
        if let Some(cached_content) = self.file_contents.get(path) {
            if self.is_valid_by_strategy(cached_content.cached_at) && cached_content.file_size == expected_size {
                self.stats.file_content_hits += 1;
                debug!(
                    "Cache HIT: contenuto file {} ({} bytes)",
                    path,
                    cached_content.data.len()
                );
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
        if data.len() > self.config.max_file_cache_size {
            debug!(
                "File {} troppo grande per cache ({} bytes)",
                path,
                data.len()
            );
            return false;
        }

        self.apply_cache_strategy();

        let cached_content = CachedFileContent::new(data, file_size, self.config.file_content_ttl);
        debug!(
            "Cache STORE: contenuto file {} ({} bytes)",
            path,
            cached_content.data.len()
        );
        self.file_contents.insert(path, cached_content);
        true
    }


    pub fn invalidate_path(&mut self, path: &str) {
        debug!("Cache INVALIDATE: path {}", path);

        self.metadata_by_path.remove(path);

        self.file_contents.remove(path);

        if let Some((ino, _)) = self.path_to_inode.remove(path) {
            self.inode_to_path.remove(&ino);
            self.metadata_by_inode.remove(&ino);
        }

        if let Some(parent_path) = path.rsplitn(2, '/').nth(1) {
            let parent = if parent_path.is_empty() {
                "/"
            } else {
                parent_path
            };
            self.directories.remove(parent);
        }

        self.stats.invalidations += 1;
    }

    pub fn invalidate_directory(&mut self, dir_path: &str) {
        debug!("Cache INVALIDATE: directory {}", dir_path);
        self.directories.remove(dir_path);
        self.stats.invalidations += 1;
    }


    pub fn cleanup_expired(&mut self) {
        let initial_count = self.metadata_by_path.len()
            + self.directories.len()
            + self.file_contents.len()
            + self.inode_to_path.len();

        self.metadata_by_path.retain(|_, cached| cached.is_valid());
        self.metadata_by_inode.retain(|_, cached| cached.is_valid());

        self.directories.retain(|_, cached| cached.is_valid());

        self.file_contents.retain(|_, cached| cached.is_valid());

        let ttl = self.config.inode_path_ttl;
        self.inode_to_path
            .retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);
        self.path_to_inode
            .retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);

        let final_count = self.metadata_by_path.len()
            + self.directories.len()
            + self.file_contents.len()
            + self.inode_to_path.len();

        if initial_count > final_count {
            info!(
                "Cache cleanup: rimossi {} elementi scaduti",
                initial_count - final_count
            );
        }
    }

    fn cleanup_expired_with_ttl(&mut self, ttl: Duration) {
        let initial_count = self.metadata_by_path.len()
            + self.directories.len()
            + self.file_contents.len()
            + self.inode_to_path.len();

        self.metadata_by_path.retain(|_, cached| cached.cached_at.elapsed() < ttl);
        self.metadata_by_inode.retain(|_, cached| cached.cached_at.elapsed() < ttl);

        self.directories.retain(|_, cached| cached.cached_at.elapsed() < ttl);

        self.file_contents.retain(|_, cached| cached.cached_at.elapsed() < ttl);

        self.inode_to_path
            .retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);
        self.path_to_inode
            .retain(|_, (_, cached_at)| cached_at.elapsed() < ttl);

        let final_count = self.metadata_by_path.len()
            + self.directories.len()
            + self.file_contents.len()
            + self.inode_to_path.len();

        if initial_count > final_count {
            info!(
                "Cache cleanup (TTL {}s): rimossi {} elementi scaduti",
                ttl.as_secs(),
                initial_count - final_count
            );
        }
    }

    pub fn get_stats(&self) -> String {
        let mut output = String::new();
        output.push_str("=== CACHE STATISTICS ===\n");

        let metadata_total = self.stats.metadata_hits + self.stats.metadata_misses;
        let directory_total = self.stats.directory_hits + self.stats.directory_misses;
        let inode_path_total = self.stats.inode_path_hits + self.stats.inode_path_misses;
        let file_content_total = self.stats.file_content_hits + self.stats.file_content_misses;

        if metadata_total > 0 {
            output.push_str(&format!(
                "Metadata: {} hits, {} misses ({:.1}% hit rate)\n",
                self.stats.metadata_hits,
                self.stats.metadata_misses,
                self.stats.metadata_hits as f64 / metadata_total as f64 * 100.0
            ));
        } else {
            output.push_str("Metadata: nessuna operazione registrata\n");
        }

        if directory_total > 0 {
            output.push_str(&format!(
                "Directory: {} hits, {} misses ({:.1}% hit rate)\n",
                self.stats.directory_hits,
                self.stats.directory_misses,
                self.stats.directory_hits as f64 / directory_total as f64 * 100.0
            ));
        } else {
            output.push_str("Directory: nessuna operazione registrata\n");
        }

        if inode_path_total > 0 {
            output.push_str(&format!(
                "Inode-Path: {} hits, {} misses ({:.1}% hit rate)\n",
                self.stats.inode_path_hits,
                self.stats.inode_path_misses,
                self.stats.inode_path_hits as f64 / inode_path_total as f64 * 100.0
            ));
        } else {
            output.push_str("Inode-Path: nessuna operazione registrata\n");
        }

        if file_content_total > 0 {
            output.push_str(&format!(
                "File Content: {} hits, {} misses ({:.1}% hit rate)\n",
                self.stats.file_content_hits,
                self.stats.file_content_misses,
                self.stats.file_content_hits as f64 / file_content_total as f64 * 100.0
            ));
        } else {
            output.push_str("File Content: nessuna operazione registrata\n");
        }

        output.push_str(&format!(
            "Total invalidations: {}\n",
            self.stats.invalidations
        ));
        output.push_str(&format!(
            "Current cache sizes: {} metadata, {} directories, {} file contents\n",
            self.metadata_by_path.len(),
            self.directories.len(),
            self.file_contents.len()
        ));

        output
    }

    pub fn apply_cache_strategy(&mut self) {
        match &self.strategy {
            CacheInvalidationStrategy::TTL(duration) => {
                self.cleanup_expired_with_ttl(*duration);
            }
            CacheInvalidationStrategy::LRU(max_entries) => {
                let max = *max_entries;
                while self.total_cached_items() > max {
                    self.evict_oldest_item();
                }
            }
        }
    }

    fn total_cached_items(&self) -> usize {
        self.metadata_by_path.len() + 
        self.directories.len() + 
        self.file_contents.len() + 
        self.inode_to_path.len()  
    }

    fn evict_oldest_item(&mut self) {
        let mut oldest_time = Instant::now();
        let mut oldest_type: Option<&str> = None;
        let mut oldest_key = String::new();
        let mut oldest_inode: Option<u64> = None;

        for (path, cached) in &self.metadata_by_path {
            if cached.cached_at < oldest_time {
                oldest_time = cached.cached_at;
                oldest_type = Some("metadata");
                oldest_key = path.clone();
            }
        }

        for (path, cached) in &self.directories {
            if cached.cached_at < oldest_time {
                oldest_time = cached.cached_at;
                oldest_type = Some("directory");
                oldest_key = path.clone();
            }
        }

        for (path, cached) in &self.file_contents {
            if cached.cached_at < oldest_time {
                oldest_time = cached.cached_at;
                oldest_type = Some("file_content");
                oldest_key = path.clone();
            }
        }

        for (ino, (path, cached_at)) in &self.inode_to_path {
            if *cached_at < oldest_time {
                oldest_time = *cached_at;
                oldest_type = Some("inode_mapping");
                oldest_key = path.clone();
                oldest_inode = Some(*ino);
            }
        }

        match oldest_type {
            Some("metadata") => {
                self.invalidate_path(&oldest_key);
                debug!("LRU evict: metadata {}", oldest_key);
            }
            Some("directory") => {
                self.directories.remove(&oldest_key);
                debug!("LRU evict: directory {}", oldest_key);
            }
            Some("file_content") => {
                self.file_contents.remove(&oldest_key);
                debug!("LRU evict: file content {}", oldest_key);
            }
            Some("inode_mapping") => {
                if let Some(ino) = oldest_inode {
                    self.inode_to_path.remove(&ino);
                    self.path_to_inode.remove(&oldest_key);
                    debug!("LRU evict: inode mapping {} <-> {}", ino, oldest_key);
                }
            }
            _ => {} 
        }
    }
}
