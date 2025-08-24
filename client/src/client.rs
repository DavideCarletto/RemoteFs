use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use std::time::Duration;

use crate::cache::FileSystemCache;
use crate::types::{FileMetadata, RemoteFsFileType};

const CHUNK_SIZE: usize = 64 * 1024;

pub struct RemoteFsClient {
    api_url: String,
    http_client: Client,
    cache: FileSystemCache,
}

impl RemoteFsClient {
    pub fn new(api_url: String) -> Self {
        let cache = FileSystemCache::with_default();

        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Errore creazione HTTP client");

        Self {
            api_url,
            http_client,
            cache,
        }
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }

    pub fn cache(&self) -> &FileSystemCache {
        &self.cache
    }

    //ottiene riferimento mutabile a cache
    pub fn cache_mut(&mut self) -> &mut FileSystemCache {
        &mut self.cache
    }

    pub fn normalize_path_for_server(&self, path: &str) -> String {
        #[cfg(target_os = "windows")]
        {
            if path == "\\" || path == "/" {
                return "/".to_string();
            }
            let replaced = path.replace("\\", "/");
            let collapsed = replaced
                .split('/')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("/");
            format!("/{}", collapsed)
        }

        #[cfg(target_os = "linux")]
        {
            path.to_string()
        }
    }

    pub fn test_connection(&self) -> Result<(), String> {
        let health_url = format!("{}/health", self.api_url);

        match self.http_client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => Err(format!("Errore connessione al server: {}", resp.status())),
            Err(e) => Err(format!("Errore di rete: {}", e)),
        }
    }

    #[cfg(target_os = "linux")]
    /// Risolve un inode in percorso tramite chiamata HTTP al server
    pub fn inode_to_path(&self, ino: u64) -> Option<String> {
        debug!("Risoluzione inode {} in percorso via HTTP", ino);
        if ino == 1 {
            info!("Inode {} risolto in percorso: /", ino);
            return Some("/".to_string());
        }

        let url = format!("{}/resolve-inode/{}", self.api_url, ino);
        match self.http_client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => match resp.text() {
                Ok(path) => {
                    info!("Inode {} risolto in percorso: {}", ino, path);
                    Some(self.normalize_path_for_server(&path))
                }
                Err(e) => {
                    error!("Errore lettura risposta per inode {}: {}", ino, e);
                    None
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("Inode {} non trovato sul server", ino);
                None
            }
            Ok(resp) => {
                error!("Errore server per inode {}: {}", ino, resp.status());
                None
            }
            Err(e) => {
                error!("Errore di rete per inode {}: {}", ino, e);
                None
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn build_path(&self, parent: u64, name: &str) -> Option<String> {
        let parent_path = self.inode_to_path(parent)?;
        let normalized_parent = self.normalize_path_for_server(&parent_path);

        if normalized_parent == "/" {
            Some(format!("/{}", name))
        } else {
            Some(format!("{}/{}", normalized_parent, name))
        }
    }

    /// Richiede i metadati di un file al server
    pub fn get_file_metadata(&mut self, path: &str) -> Option<FileMetadata> {
        let server_path = self.normalize_path_for_server(path);
        let url = format!("{}/metadata?path={}", self.api_url, server_path);

        match self.http_client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                Ok(metadata) => {
                    info!(
                        "Metadati ricevuti per {}: inode {}",
                        server_path, metadata.ino
                    );
                    Some(metadata)
                }
                Err(e) => {
                    error!("Errore parsing JSON per {}: {}", server_path, e);
                    None
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato: {}", server_path);
                None
            }
            Ok(resp) => {
                error!("Errore server per {}: {}", server_path, resp.status());
                None
            }
            Err(e) => {
                error!("Errore di rete per {}: {}", server_path, e);
                None
            }
        }
    }

    /// Aggiorna gli attributi di un file
    pub fn update_file_metadata(
        &self,
        path: &str,
        updates: serde_json::Value,
    ) -> Option<FileMetadata> {
        let server_path = self.normalize_path_for_server(path);

        debug!(
            "Aggiornamento attributi per: {} con {:?}",
            server_path, updates
        );

        let url = format!("{}/metadata?path={}", self.api_url, server_path);

        match self.http_client.patch(&url).json(&updates).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                Ok(metadata) => {
                    info!(
                        "Attributi aggiornati per {}: inode {}",
                        server_path, metadata.ino
                    );
                    Some(metadata)
                }
                Err(e) => {
                    error!("Errore parsing JSON per {}: {}", server_path, e);
                    None
                }
            },
            Ok(resp) => {
                error!("Errore server per {}: {}", server_path, resp.status());
                None
            }
            Err(e) => {
                error!("Errore di rete per {}: {}", server_path, e);
                None
            }
        }
    }

    pub fn create_filesystem_object(
        &self,
        request_data: serde_json::Value,
    ) -> Result<FileMetadata, i32> {
        let file_type = request_data
            .get("file_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("RegularFile");

        let url = if file_type == "Directory" {
            format!("{}/mkdir", self.api_url)
        } else {
            format!("{}/files", self.api_url)
        };

        match self.http_client.post(&url).json(&request_data).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                Ok(metadata) => {
                    info!("{} creato (server-side): inode {}", file_type, metadata.ino);
                    Ok(metadata)
                }
                Err(e) => {
                    error!("Parsing JSON fallito in create_filesystem_object: {}", e);
                    Err(libc::EIO)
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                warn!("Conflitto creazione oggetto filesystem: già esistente");
                Err(libc::EEXIST)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("Directory padre non trovata");
                Err(libc::ENOENT)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                warn!("Permessi insufficienti per creazione oggetto filesystem");
                Err(libc::EACCES)
            }
            Ok(resp) => {
                error!(
                    "Errore server nella creazione oggetto filesystem: {}",
                    resp.status()
                );
                Err(libc::EIO)
            }
            Err(e) => {
                error!("Errore di rete nella creazione oggetto filesystem: {}", e);
                Err(libc::EIO)
            }
        }
    }

    /// Rimuove un filesystem object (file, directory, etc.) dal server tramite chiamata HTTP
    pub fn remove_filesystem_object(&mut self, path: &str, is_directory: bool) -> Result<(), i32> {
        let server_path = self.normalize_path_for_server(path);

        debug!(
            "Rimozione filesystem object: {} (directory: {})",
            server_path, is_directory
        );

        if let None = self.get_file_metadata(&server_path) {
            error!("File non trovato per rimozione: {}", server_path);
            return Err(libc::ENOENT);
        }

        let url = format!("{}/files", self.api_url);

        match self
            .http_client
            .delete(&url)
            .query(&[("path", server_path.clone()), ("is_directory", is_directory.to_string())])
            .send()
        {
            Ok(resp) => match resp.status() {
                reqwest::StatusCode::OK => {
                    info!("Filesystem object rimosso: {}", server_path);
                    Ok(())
                }
                reqwest::StatusCode::NOT_FOUND => {
                    warn!("File non trovato per rimozione: {}", server_path);
                    Err(libc::ENOENT)
                }
                reqwest::StatusCode::FORBIDDEN => {
                    warn!("Permessi insufficienti per rimuovere: {}", server_path);
                    Err(libc::EACCES)
                }
                reqwest::StatusCode::CONFLICT => {
                    if let Ok(response_text) = resp.text() {
                        if response_text.contains("directory not empty") {
                            warn!("Directory non vuota: {}", server_path);
                            Err(libc::ENOTEMPTY)
                        } else {
                            warn!("File in uso: {}", server_path);
                            Err(libc::EBUSY)
                        }
                    } else {
                        warn!("Directory non vuota o file in uso: {}", server_path);
                        Err(libc::ENOTEMPTY)
                    }
                }
                _ => {
                    error!(
                        "Errore server in rimozione per {}: {}",
                        server_path,
                        resp.status()
                    );
                    Err(libc::EIO)
                }
            },
            Err(e) => {
                if e.is_timeout() {
                    error!("Timeout durante la rimozione di {}: {}", server_path, e);
                    Err(libc::ETIMEDOUT)
                } else if e.is_connect() {
                    error!(
                        "Errore di connessione durante la rimozione di {}: {}",
                        server_path, e
                    );
                    Err(libc::ECONNREFUSED)
                } else {
                    error!("Errore di rete in rimozione per {}: {}", server_path, e);
                    Err(libc::EIO)
                }
            }
        }
    }

    /// Apre un file e restituisce un file handle
    pub fn open_file(&mut self, path: &str, flags: i32) -> Result<u64, i32> {
        debug!("Apertura file: {} con flags: {:#x}", path, flags);
        let server_path = self.normalize_path_for_server(path);

        if let Some(metadata) = self.get_file_metadata(&server_path) {
            if metadata.file_type == RemoteFsFileType::Directory {
                debug!("Directory {} aperta", server_path);
                let dir_handle = metadata.ino;
                return Ok(dir_handle);
            }
        }

        let url = format!("{}/open", self.api_url);

        let open_data = serde_json::json!({
            "path": server_path,
            "flags": flags
        });

        match self.http_client.post(&url).json(&open_data).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>() {
                Ok(response) => {
                    if let Some(fh) = response.get("file_handle").and_then(|v| v.as_u64()) {
                        info!("File aperto: {} -> file handle {}", server_path, fh);
                        Ok(fh)
                    } else {
                        error!(
                            "Risposta server non valida per apertura {}: manca file_handle",
                            server_path
                        );
                        Err(libc::EIO)
                    }
                }
                Err(e) => {
                    error!("Errore parsing JSON in apertura per {}: {}", server_path, e);
                    Err(libc::EIO)
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato per apertura: {}", server_path);
                Err(libc::ENOENT)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                warn!("Permessi insufficienti per aprire: {}", server_path);
                Err(libc::EACCES)
            }
            Ok(resp) => {
                error!(
                    "Errore server in apertura per {}: {}",
                    server_path,
                    resp.status()
                );
                Err(libc::EIO)
            }
            Err(e) => {
                error!("Errore di rete in apertura per {}: {}", server_path, e);
                Err(libc::EIO)
            }
        }
    }

    /// Legge dati da un file sul server tramite chiamata HTTP con streaming
    pub fn read_file(
        &mut self,
        path: &str,
        file_handle: u64,
        offset: i64,
        size: u32,
    ) -> Result<Vec<u8>, i32> {
        let server_path = self.normalize_path_for_server(path);

        if offset >= 0 {
            if let Some(metadata) = self.get_file_metadata(&server_path) {
                if offset as u64 >= metadata.size {
                    debug!(
                        "EOF raggiunto: offset {} >= file size {}",
                        offset, metadata.size
                    );
                    return Ok(Vec::new());
                }

                let remaining = metadata.size - offset as u64;
                let read_size = size.min(remaining as u32);

                if read_size == 0 {
                    return Ok(Vec::new());
                }

                if offset == 0 && read_size <= 1024 * 1024 {
                    if let Some(cached_data) =
                        self.cache.get_file_content(&server_path, metadata.size)
                    {
                        let end = (read_size as usize).min(cached_data.len());
                        debug!("Dati file trovati in cache per: {}", server_path);
                        return Ok(cached_data[..end].to_vec());
                    }
                }

                debug!(
                    "Lettura file in streaming: {} (fh: {}, offset: {}, size: {})",
                    server_path, file_handle, offset, read_size
                );

                let mut result = Vec::new();
                let mut current_offset = offset;
                let mut remaining_size = read_size;
                let mut chunk_index = 0;

                info!(
                    "Inizio lettura streaming: offset={}, size={} bytes, chunk_size={}KB",
                    offset,
                    read_size,
                    CHUNK_SIZE / 1024
                );

                while remaining_size > 0 {
                    let current_chunk_size = std::cmp::min(remaining_size, CHUNK_SIZE as u32);

                    debug!(
                        "Lettura chunk {}: offset={}, size={} bytes",
                        chunk_index, current_offset, current_chunk_size
                    );

                    let url = format!("{}/files", self.api_url);

                    let request_data = serde_json::json!({
                        "path": server_path,
                        "file_handle": file_handle,
                        "offset": current_offset,
                        "size": current_chunk_size,
                        "chunk_index": chunk_index
                    });

                    match self.http_client.get(&url).json(&request_data).send() {
                        Ok(resp) if resp.status().is_success() => match resp.bytes() {
                            Ok(chunk_data) => {
                                result.extend_from_slice(&chunk_data);
                                current_offset += current_chunk_size as i64;
                                remaining_size -= current_chunk_size;
                                chunk_index += 1;

                                if chunk_index % 100 == 0 {
                                    let progress = ((read_size - remaining_size) as f64
                                        / read_size as f64)
                                        * 100.0;
                                    info!(
                                        "Progresso lettura streaming: {:.1}% ({}/{} bytes)",
                                        progress,
                                        read_size - remaining_size,
                                        read_size
                                    );
                                }
                            }
                            Err(e) => {
                                error!("Errore lettura chunk {} per {}: {}", chunk_index, path, e);
                                return Err(libc::EIO);
                            }
                        },
                        Ok(resp) => {
                            error!(
                                "Errore server lettura chunk {} per {}: {}",
                                chunk_index,
                                server_path,
                                resp.status()
                            );
                            return Err(libc::EIO);
                        }
                        Err(e) => {
                            error!(
                                "Errore rete lettura chunk {} per {}: {}",
                                chunk_index, server_path, e
                            );
                            return Err(libc::EIO);
                        }
                    }
                }

                debug!(
                    "Lettura streaming completata: {} chunks totali, {} bytes totali",
                    chunk_index,
                    result.len()
                );
                return Ok(result);
            }
        }
        Err(libc::ENOENT)
    }

    /// Lista il contenuto di una directory dal server (versione generica)
    pub fn list_directory(&self, path: &str) -> Result<Vec<(String, u64, RemoteFsFileType)>, i32> {
        let server_path = self.normalize_path_for_server(path);
        debug!("Lista directory: {}", server_path);

        let response = self
            .http_client
            .get(&format!("{}/list", self.api_url))
            .query(&[("path", server_path.clone())])
            .send();

        match response {
            Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>() {
                Ok(data) => {
                    if let Some(entries) = data.get("entries").and_then(|v| v.as_array()) {
                        let mut result = Vec::new();

                        for entry in entries {
                            if let (Some(name), Some(ino), Some(file_type)) = (
                                entry.get("name").and_then(|v| v.as_str()),
                                entry.get("ino").and_then(|v| v.as_u64()),
                                entry.get("file_type").and_then(|v| v.as_str()),
                            ) {
                                let file_type = match file_type {
                                    "Directory" => RemoteFsFileType::Directory,
                                    "RegularFile" => RemoteFsFileType::RegularFile,
                                    _ => RemoteFsFileType::RegularFile,
                                };
                                result.push((name.to_string(), ino, file_type));
                            }
                        }

                        info!(
                            "Directory {} contiene {} elementi",
                            &server_path,
                            result.len()
                        );
                        Ok(result)
                    } else {
                        error!(
                            "Risposta server non valida per listdir {}: manca entries",
                            server_path
                        );
                        Err(libc::ENOENT)
                    }
                }
                Err(e) => {
                    error!("Errore parsing JSON per listdir {}: {}", server_path, e);
                    Err(libc::EIO)
                }
            },
            Ok(resp) => {
                error!(
                    "Errore server in listdir per {}: {}",
                    server_path,
                    resp.status()
                );
                Err(libc::EIO)
            }
            Err(e) => {
                error!("Errore di rete in listdir per {}: {}", server_path, e);
                Err(libc::EIO)
            }
        }
    }

    /// Rinomina/sposta un file o directory sul server tramite chiamata HTTP
    pub fn rename_filesystem_object(&self, old_path: &str, new_path: &str) -> Result<(), i32> {
        let old_server_path = self.normalize_path_for_server(old_path);
        let new_server_path = self.normalize_path_for_server(new_path);
        debug!(
            "Rinomina filesystem object: {} -> {}",
            old_server_path, new_server_path
        );

        let url = format!("{}/rename", self.api_url);

        let rename_data = serde_json::json!({
            "old_path": old_server_path,
            "new_path": new_server_path
        });

        match self.http_client.post(&url).json(&rename_data).send() {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    "Filesystem object rinominato: {} -> {}",
                    old_server_path, new_server_path
                );
                Ok(())
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato per rinomina: {}", old_server_path);
                Err(libc::ENOENT)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                warn!("File destinazione già esistente: {}", new_server_path);
                Err(libc::EEXIST)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::BAD_REQUEST => {
                warn!(
                    "Operazione di rinomina non valida: {} -> {}",
                    old_server_path, new_server_path
                );
                Err(libc::EINVAL)
            }
            Ok(resp) => {
                error!(
                    "Errore server in rinomina per {} -> {}: {}",
                    old_server_path,
                    new_server_path,
                    resp.status()
                );
                Err(libc::EIO)
            }
            Err(e) => {
                error!(
                    "Errore di rete in rinomina per {} -> {}: {}",
                    old_server_path, new_server_path, e
                );
                Err(libc::EIO)
            }
        }
    }

    /// Scrive dati in un file sul server tramite chiamata HTTP con streaming
    pub fn write_file(
        &self,
        path: &str,
        file_handle: u64,
        offset: i64,
        data: &[u8],
    ) -> Result<u32, i32> {
        let server_path = self.normalize_path_for_server(path);
        debug!(
            "Scrittura file in streaming: {} (fh: {}, offset: {}, size: {})",
            server_path,
            file_handle,
            offset,
            data.len()
        );

        let chunks: Vec<&[u8]> = data.chunks(CHUNK_SIZE).collect();
        let total_chunks = chunks.len();
        let mut total_written = 0u32;

        info!(
            "Inizio scrittura streaming: {} chunks da {}KB ciascuno",
            total_chunks,
            CHUNK_SIZE / 1024
        );

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            let chunk_offset = offset + (chunk_index * CHUNK_SIZE) as i64;

            debug!(
                "Scrittura chunk {}/{} ({}KB)",
                chunk_index + 1,
                total_chunks,
                chunk.len() / 1024
            );

            let url = format!("{}/files", self.api_url);
            let response = self
                .http_client
                .put(&url)
                .header("Content-Type", "application/octet-stream")
                .header("X-Path", &server_path)
                .header("X-File-Handle", file_handle.to_string())
                .header("X-Offset", chunk_offset.to_string())
                .body(chunk.to_vec())
                .send();

            match response {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>() {
                        Ok(response_data) => {
                            if let Some(bytes_written) =
                                response_data.get("bytes_written").and_then(|v| v.as_u64())
                            {
                                total_written += bytes_written as u32;

                                // Progress feedback ogni 50 chunk
                                if chunk_index % 50 == 0 && total_chunks > 1 {
                                    let progress =
                                        ((chunk_index + 1) as f32 / total_chunks as f32) * 100.0;
                                    info!(
                                        "Chunk {}/{} completato - Progresso: {:.1}%",
                                        chunk_index + 1,
                                        total_chunks,
                                        progress
                                    );
                                }
                            } else {
                                error!(
                                    "Risposta server non valida per chunk {}: manca bytes_written",
                                    chunk_index
                                );
                                return Err(libc::EIO);
                            }
                        }
                        Err(e) => {
                            error!("Errore parsing JSON per chunk {}: {}", chunk_index, e);
                            return Err(libc::EIO);
                        }
                    }
                }
                Ok(resp) => {
                    error!("Errore server per chunk {}: {}", chunk_index, resp.status());
                    return Err(libc::EIO);
                }
                Err(e) => {
                    error!("Errore di rete per chunk {}: {}", chunk_index, e);
                    return Err(libc::EIO);
                }
            }
        }

        info!(
            "Scrittura streaming completata: {} bytes scritti in {} chunks",
            total_written, total_chunks
        );
        Ok(total_written)
    }
}
