#[cfg(target_os = "linux")]
mod linux_filesystem {

use fuser::{FileAttr, FileType, Filesystem};
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::{Duration, SystemTime};

const MAX_NAME_LENGTH: u32 = 255;
const CHUNK_SIZE: usize = 64 * 1024;

pub struct RemoteFsClient {
    api_url: String,
}

impl RemoteFsClient {
    pub fn new(api_url: String) -> Self {
        Self { api_url }
    }

    /// Risolve un inode in percorso tramite chiamata HTTP al server
    fn inode_to_path(&self, ino: u64) -> Option<String> {
        debug!("Risoluzione inode {} in percorso via HTTP", ino);

        // Caso speciale: root directory
        if ino == 1 {
            info!("Inode {} risolto in percorso: /", ino);
            return Some("/".to_string());
        }

        // Chiamata HTTP per risolvere inode -> path
        let client = Client::new();
        let url = format!("{}/resolve-inode/{}", self.api_url, ino);

        match client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => match resp.text() {
                Ok(path) => {
                    info!("Inode {} risolto in percorso: {}", ino, path);
                    Some(path)
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

    /// Costruisce il percorso completo da parent inode + nome
    fn build_path(&self, parent: u64, name: &str) -> Option<String> {
        let parent_path = self.inode_to_path(parent)?;

        if parent_path == "/" {
            Some(format!("/{}", name))
        } else {
            Some(format!("{}/{}", parent_path, name))
        }
    }

    /// Richiede i metadati di un file al server
    fn get_file_metadata(&self, path: &str) -> Option<FileMetadata> {
        let client = Client::new();
        let url = format!("{}/metadata?path={}", self.api_url, path);

        match client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                Ok(metadata) => {
                    info!("Metadati ricevuti per {}: inode {}", path, metadata.ino);
                    Some(metadata)
                }
                Err(e) => {
                    error!("Errore parsing JSON per {}: {}", path, e);
                    None
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato: {}", path);
                None
            }
            Ok(resp) => {
                error!("Errore server per {}: {}", path, resp.status());
                None
            }
            Err(e) => {
                error!("Errore di rete per {}: {}", path, e);
                None
            }
        }
    }

    fn update_file_attributes(
        &self,
        path: &str,
        updates: serde_json::Value,
    ) -> Option<FileMetadata> {
        debug!("Aggiornamento attributi per: {} con {:?}", path, updates);

        let client = Client::new();
        let url = format!("{}/metadata?path={}", self.api_url, path);

        match client.patch(&url).json(&updates).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                Ok(metadata) => {
                    info!("Attributi aggiornati per {}: inode {}", path, metadata.ino);
                    Some(metadata)
                }
                Err(e) => {
                    error!("Errore parsing JSON per {}: {}", path, e);
                    None
                }
            },
            Ok(resp) => {
                error!("Errore server per {}: {}", path, resp.status());
                None
            }
            Err(e) => {
                error!("Errore di rete per {}: {}", path, e);
                None
            }
        }
    }

    /// Crea un nuovo filesystem object (file, directory, etc.) sul server tramite chiamata HTTP
    fn create_filesystem_object(
        &self,
        path: &str,
        file_type: &str,
        mode: u32,
        uid: u32,
        gid: u32,
        rdev: u32,
        umask: u32,
    ) -> Result<FileMetadata, i32> {
        debug!("Creazione filesystem object: {} tipo: {} mode: {:o} uid: {} gid: {}", 
            path, file_type, mode, uid, gid);

        let client = Client::new();
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        if file_type == "Directory" {
            // Per le directory usiamo POST /mkdir
            let url = format!("{}/mkdir", self.api_url);
            let request_data = serde_json::json!({
                "path": path,
                "file_type": file_type,
                "mode": mode,
                "uid": uid,
                "gid": gid,
                "rdev": rdev,
                "umask": umask,
                "atime": current_time,
                "mtime": current_time,
                "ctime": current_time,
                "crtime": current_time
            });

            match client.post(&url).json(&request_data).send() {
                Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                    Ok(metadata) => {
                        info!("Directory creata: {} -> inode {}", path, metadata.ino);
                        Ok(metadata)
                    }
                    Err(e) => {
                        error!(
                            "Errore parsing JSON in creazione directory per {}: {}",
                            path, e
                        );
                        Err(libc::EIO)
                    }
                },
                Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                    warn!("Directory già esistente: {}", path);
                    Err(libc::EEXIST)
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                    warn!("Directory padre non trovata per: {}", path);
                    Err(libc::ENOENT)
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                    warn!("Permessi insufficienti per creare directory: {}", path);
                    Err(libc::EACCES)
                }
                Ok(resp) => {
                    error!(
                        "Errore server in creazione directory per {}: {}",
                        path,
                        resp.status()
                    );
                    Err(libc::EIO)
                }
                Err(e) => {
                    error!("Errore di rete in creazione directory per {}: {}", path, e);
                    Err(libc::EIO)
                }
            }
        } else {
            // Per i file regolari usiamo POST /files
            let url = format!("{}/files", self.api_url);
            let effective_mode = mode & !umask;  // Applica umask ai permessi
            info!("Creazione file con permessi: {:o} (mode: {:o}, umask: {:o})", 
                effective_mode, mode, umask);
                
            let request_data = serde_json::json!({
                "path": path,
                "file_type": file_type,
                "mode": effective_mode,
                "uid": uid,
                "gid": gid,
                "rdev": rdev,
                "size": 0,
                "permissions": effective_mode,
                "atime": current_time,
                "mtime": current_time,
                "ctime": current_time,
                "crtime": current_time,
                "blocks": 0,
                "blksize": 512,
                "nlink": 1
            });

            match client.post(&url).json(&request_data).send() {
                Ok(resp) if resp.status().is_success() => match resp.json::<FileMetadata>() {
                    Ok(metadata) => {
                        info!("File creato: {} -> inode {}", path, metadata.ino);
                        Ok(metadata)
                    }
                    Err(e) => {
                        error!("Errore parsing JSON in creazione per {}: {}", path, e);
                        Err(libc::EIO)
                    }
                },
                Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                    warn!("File già esistente: {}", path);
                    Err(libc::EEXIST)
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                    warn!("Directory padre non trovata per: {}", path);
                    Err(libc::ENOENT)
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                    warn!("Permessi insufficienti per creare: {}", path);
                    Err(libc::EACCES)
                }
                Ok(resp) => {
                    error!("Errore server in creazione per {}: {}", path, resp.status());
                    Err(libc::EIO)
                }
                Err(e) => {
                    error!("Errore di rete in creazione per {}: {}", path, e);
                    Err(libc::EIO)
                }
            }
        }
    }

    /// Rimuove un filesystem object (file, directory, etc.) dal server tramite chiamata HTTP
    fn remove_filesystem_object(&self, path: &str, is_directory: bool) -> Result<(), i32> {
        debug!(
            "Rimozione filesystem object: {} (directory: {})",
            path, is_directory
        );

        // Verifica prima se il file esiste
        if let None = self.get_file_metadata(path) {
            error!("File non trovato per rimozione: {}", path);
            return Err(libc::ENOENT);
        }

        let client = Client::new();
        let url = format!("{}/files", self.api_url);

        match client
            .delete(&url)
            .query(&[("path", path), ("is_directory", &is_directory.to_string())])
            .send()
        {
            Ok(resp) => {
                match resp.status() {
                    reqwest::StatusCode::OK => {
                        info!("Filesystem object rimosso: {}", path);
                        Ok(())
                    }
                    reqwest::StatusCode::NOT_FOUND => {
                        warn!("File non trovato per rimozione: {}", path);
                        Err(libc::ENOENT)
                    }
                    reqwest::StatusCode::FORBIDDEN => {
                        warn!("Permessi insufficienti per rimuovere: {}", path);
                        Err(libc::EACCES)
                    }
                    reqwest::StatusCode::CONFLICT => {
                        // Distingui tra directory non vuota e file in uso
                        if let Ok(response_text) = resp.text() {
                            if response_text.contains("directory not empty") {
                                warn!("Directory non vuota: {}", path);
                                Err(libc::ENOTEMPTY)
                            } else {
                                warn!("File in uso: {}", path);
                                Err(libc::EBUSY)
                            }
                        } else {
                            warn!("Directory non vuota o file in uso: {}", path);
                            Err(libc::ENOTEMPTY)
                        }
                    }
                    _ => {
                        error!("Errore server in rimozione per {}: {}", path, resp.status());
                        Err(libc::EIO)
                    }
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    error!("Timeout durante la rimozione di {}: {}", path, e);
                    Err(libc::ETIMEDOUT)
                } else if e.is_connect() {
                    error!("Errore di connessione durante la rimozione di {}: {}", path, e);
                    Err(libc::ECONNREFUSED)
                } else {
                    error!("Errore di rete in rimozione per {}: {}", path, e);
                    Err(libc::EIO)
                }
            }
        }
    }

    fn open_file(&self, path: &str, flags: i32) -> Result<u64, i32> {
        debug!("Apertura file: {} con flags: {:#x}", path, flags);

        let client = Client::new();
        let url = format!("{}/open", self.api_url);

        let open_data = serde_json::json!({
            "path": path,
            "flags": flags
        });

        match client.post(&url).json(&open_data).send() {
            Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>() {
                Ok(response) => {
                    if let Some(fh) = response.get("file_handle").and_then(|v| v.as_u64()) {
                        info!("File aperto: {} -> file handle {}", path, fh);
                        Ok(fh)
                    } else {
                        error!(
                            "Risposta server non valida per apertura {}: manca file_handle",
                            path
                        );
                        Err(libc::EIO)
                    }
                }
                Err(e) => {
                    error!("Errore parsing JSON in apertura per {}: {}", path, e);
                    Err(libc::EIO)
                }
            },
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato per apertura: {}", path);
                Err(libc::ENOENT)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                warn!("Permessi insufficienti per aprire: {}", path);
                Err(libc::EACCES)
            }
            Ok(resp) => {
                error!("Errore server in apertura per {}: {}", path, resp.status());
                Err(libc::EIO)
            }
            Err(e) => {
                error!("Errore di rete in apertura per {}: {}", path, e);
                Err(libc::EIO)
            }
        }
    }

    /// Legge dati da un file sul server tramite chiamata HTTP con streaming
    fn read_file(
        &self,
        path: &str,
        file_handle: u64,
        offset: i64,
        size: u32,
    ) -> Result<Vec<u8>, i32> {
        #[cfg(feature = "cache")]
        if offset == 0 && size <= 1024 * 1024 {
            if let Some(metadata) = self.get_file_metadata(path) {
                if let Ok(mut cache) = self.cache.lock() {
                    if let Some(cached_data) = cache.get_file_content(path, metadata.size) {
                        let end = (size as usize).min(cached_data.len());
                        return Ok(cached_data[..end].to_vec());
                    }
                }
            }
        }

        debug!(
            "Lettura file in streaming: {} (fh: {}, offset: {}, size: {})",
            path, file_handle, offset, size
        );

        let mut result = Vec::new();
        let mut current_offset = offset;
        let mut remaining_size = size;
        let mut chunk_index = 0;

        info!(
            "Inizio lettura streaming: offset={}, size={} bytes, chunk_size={}KB",
            offset,
            size,
            CHUNK_SIZE / 1024
        );

        while remaining_size > 0 {
            let current_chunk_size = std::cmp::min(remaining_size, CHUNK_SIZE as u32);

            debug!(
                "Lettura chunk {}: offset={}, size={} bytes",
                chunk_index, current_offset, current_chunk_size
            );

            let client = Client::new();
            let url = format!("{}/files", self.api_url);

            let request_data = serde_json::json!({
                "path": path,
                "file_handle": file_handle,
                "offset": current_offset,
                "size": current_chunk_size,
                "chunk_index": chunk_index
            });

            match client.get(&url).json(&request_data).send() {
                Ok(resp) if resp.status().is_success() => match resp.bytes() {
                    Ok(chunk_data) => {
                        result.extend_from_slice(&chunk_data);
                        current_offset += current_chunk_size as i64;
                        remaining_size -= current_chunk_size;
                        chunk_index += 1;

                        if chunk_index % 100 == 0 {
                            let progress = ((size - remaining_size) as f64 / size as f64) * 100.0;
                            info!(
                                "Progresso lettura streaming: {:.1}% ({}/{} bytes)",
                                progress,
                                size - remaining_size,
                                size
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
                        path,
                        resp.status()
                    );
                    return Err(libc::EIO);
                }
                Err(e) => {
                    error!(
                        "Errore rete lettura chunk {} per {}: {}",
                        chunk_index, path, e
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
        Ok(result)
    }

    /// Lista il contenuto di una directory dal server
    fn list_directory(&self, path: &str) -> Result<Vec<(String, u64, fuser::FileType)>, i32> {
        debug!("Lista directory: {}", path);

        let client = Client::new();
        let response = client
            .get(&format!("{}/list", self.api_url))
            .query(&[("path", path)])
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
                                let fuse_type = match file_type {
                                    "Directory" => fuser::FileType::Directory,
                                    "RegularFile" => fuser::FileType::RegularFile,
                                    _ => fuser::FileType::RegularFile,
                                };
                                result.push((name.to_string(), ino, fuse_type));
                            }
                        }

                        info!("Directory {} contiene {} elementi", path, result.len());
                        Ok(result)
                    } else {
                        error!(
                            "Risposta server non valida per listdir {}: manca entries",
                            path
                        );
                        Err(libc::ENOENT)
                    }
                }
                Err(e) => {
                    error!("Errore parsing JSON per listdir {}: {}", path, e);
                    Err(libc::EIO)
                }
            },
            Ok(resp) => {
                error!("Errore server in listdir per {}: {}", path, resp.status());
                Err(libc::EIO)
            }
            Err(e) => {
                error!("Errore di rete in listdir per {}: {}", path, e);
                Err(libc::EIO)
            }
        }
    }

    /// Rinomina/sposta un file o directory sul server tramite chiamata HTTP
    fn rename_filesystem_object(&self, old_path: &str, new_path: &str) -> Result<(), i32> {
        debug!("Rinomina filesystem object: {} -> {}", old_path, new_path);

        let client = Client::new();
        let url = format!("{}/rename", self.api_url);

        let rename_data = serde_json::json!({
            "old_path": old_path,
            "new_path": new_path
        });

        match client.post(&url).json(&rename_data).send() {
            Ok(resp) if resp.status().is_success() => {
                info!("Filesystem object rinominato: {} -> {}", old_path, new_path);
                Ok(())
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                warn!("File non trovato per rinomina: {}", old_path);
                Err(libc::ENOENT)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::CONFLICT => {
                warn!("File destinazione già esistente: {}", new_path);
                Err(libc::EEXIST)
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::BAD_REQUEST => {
                warn!(
                    "Operazione di rinomina non valida: {} -> {}",
                    old_path, new_path
                );
                Err(libc::EINVAL)
            }
            Ok(resp) => {
                error!(
                    "Errore server in rinomina per {} -> {}: {}",
                    old_path,
                    new_path,
                    resp.status()
                );
                Err(libc::EIO)
            }
            Err(e) => {
                error!(
                    "Errore di rete in rinomina per {} -> {}: {}",
                    old_path, new_path, e
                );
                Err(libc::EIO)
            }
        }
    }

    /// Scrive dati in un file sul server tramite chiamata HTTP con streaming
    fn write_file(
        &self,
        path: &str,
        file_handle: u64,
        offset: i64,
        data: &[u8],
    ) -> Result<u32, i32> {
        debug!(
            "Scrittura file in streaming: {} (fh: {}, offset: {}, size: {})",
            path,
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

            let client = Client::new();

            let url = format!("{}/files", self.api_url);
            let response = client
                .put(&url)
                .header("Content-Type", "application/octet-stream")
                .header("X-Path", path)
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

impl Filesystem for RemoteFsClient {
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        let health_url = format!("{}/health", self.api_url);
        info!("Tentativo di connessione al server: {}", health_url);
        
        let client = Client::new();
        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                config.set_max_readahead(1024 * 1024).ok(); 
                config.set_max_write(1024 * 1024).ok();      

                info!("Remote FS client initialized with 1MB chunks.");
                Ok(())
            }
            _ => {
                error!(
                    "Errore: impossibile raggiungere il server API all'URL {}",
                    health_url
                );
                Err(libc::EIO)
            }
        }
    }
    fn destroy(&mut self) {
        //stampa statistiche cache prima di distruggere
        #[cfg(feature = "cache")]
        if let Ok(cache) = self.cache.lock() {
            cache.print_stats();
        }

        info!("Filesystem remoto smontato e distrutto");
        // Puoi aggiungere cleanup qui se necessario:
        // - Chiudere connessioni HTTP persistenti
        // - Salvare cache o stato
        // - Log di chiusura
    }

    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        debug!("lookup(parent: {}, name: {:?})", parent, name);

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file non valido: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.get_file_metadata(&full_path) {
            Some(metadata) => {
                let file_attr = metadata.to_file_attr();

                info!("File trovato: {} -> inode {}", full_path, metadata.ino);
                reply.entry(&std::time::Duration::from_secs(1), &file_attr, 0);
            }
            None => {
                debug!("File non trovato: {}", full_path);
                reply.error(libc::ENOENT);
            }
        }
    }

    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _fh: Option<u64>,
        reply: fuser::ReplyAttr,
    ) {
        debug!("getattr(ino: {:#x?} )", ino);

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!("Impossibile trovare il percorso per inode {:#x?}", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        info!("Recupero metadati per path: {}", path);
        match self.get_file_metadata(&path) {
            Some(metadata) => {
                info!("Metadati trovati per {}: {:?}", path, metadata);
                let file_attr = metadata.to_file_attr();
                info!("FileAttr convertito: {:?}", file_attr);
                reply.attr(&std::time::Duration::from_secs(1), &file_attr);
            }
            None => {
                warn!("File non trovato: {}", path);
                reply.error(libc::ENOENT);
            }
        }
    }

    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        //da usare quando viene richiesto di impostare il tempo "now"
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        //logica per convertire i timestamp
        let atime_secs = match atime {
            Some(fuser::TimeOrNow::Now) => Some(current_time),
            Some(fuser::TimeOrNow::SpecificTime(time)) => Some(
                time.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0))
                    .as_secs(),
            ),
            None => None,
        };

        let mtime_secs = match mtime {
            Some(fuser::TimeOrNow::Now) => Some(current_time),
            Some(fuser::TimeOrNow::SpecificTime(time)) => Some(
                time.duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0))
                    .as_secs(),
            ),
            None => None,
        };

        let mut updates = serde_json::json!({
            "mode": mode,
            "uid": uid,
            "gid": gid,
            "size": size,
            "flags": flags,
        });

        if let Some(atime) = atime_secs {
            updates.as_object_mut().unwrap().insert("atime".to_string(), json!(atime));
        }
        if let Some(mtime) = mtime_secs {
            updates.as_object_mut().unwrap().insert("mtime".to_string(), json!(mtime));
        }

        match self.update_file_attributes(path.as_str(), updates.clone()) {
            Some(metadata) => {
                let file_attr = metadata.to_file_attr();
                reply.attr(&std::time::Duration::from_secs(1), &file_attr);
            }
            None => {
                debug!("Impossibile aggiornare attributi per {}: {}", path, updates);
                reply.error(libc::ENOENT);
            }
        }
    }

    fn mknod(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "mknod(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?}, rdev: {})",
            parent, name, mode, umask, rdev
        );

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file non valido per mknod: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per mknod: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file_type = match mode & libc::S_IFMT {
            libc::S_IFREG => "RegularFile",
            libc::S_IFDIR => "Directory",
            libc::S_IFLNK => "Symlink",
            libc::S_IFBLK => "BlockDevice",
            libc::S_IFCHR => "CharDevice",
            libc::S_IFIFO => "NamedPipe",
            libc::S_IFSOCK => "Socket",
            _ => {
                error!("Tipo di file non supportato in mknod: mode {:#o}", mode);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let permissions = mode & !libc::S_IFMT;
        match self.create_filesystem_object(
            &full_path,
            file_type,
            permissions,
            _req.uid(),
            _req.gid(),
            rdev,
            umask,
        ) {
            Ok(metadata) => {
                let file_attr = metadata.to_file_attr();
                reply.entry(&std::time::Duration::from_secs(1), &file_attr, 0);
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn mkdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "mkdir(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?})",
            parent, name, mode, umask
        );

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome directory non valido per mkdir: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per mkdir: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file_type = "Directory";
        let permissions = mode & !libc::S_IFMT;

        match self.create_filesystem_object(
            &full_path,
            file_type,
            permissions,
            _req.uid(),
            _req.gid(),
            0,
            umask,
        ) {
            Ok(metadata) => {
                let file_attr = metadata.to_file_attr();
                reply.entry(&std::time::Duration::from_secs(1), &file_attr, 0);
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("unlink(parent: {:#x?}, name: {:?})", parent, name,);

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file non valido per unlink: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per unlink: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.remove_filesystem_object(&full_path, false) {
            Ok(()) => {
                reply.ok();
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("rmdir(parent: {:#x?}, name: {:?})", parent, name,);

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome directory non valido per rmdir: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per rmdir: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.remove_filesystem_object(&full_path, true) {
            Ok(()) => {
                reply.ok();
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn rename(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        newparent: u64,
        newname: &std::ffi::OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "rename(parent: {:#x?}, name: {:?}, newparent: {:#x?}, newname: {:?}, flags: {})",
            parent, name, newparent, newname, flags,
        );

        if name.len() > MAX_NAME_LENGTH as usize || newname.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file sorgente non valido per rename: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let newname_str = match newname.to_str() {
            Some(s) => s,
            None => {
                error!(
                    "Nome file destinazione non valido per rename: {:?}",
                    newname
                );
                reply.error(libc::EINVAL);
                return;
            }
        };

        let old_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso sorgente per rename: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        let new_path = match self.build_path(newparent, newname_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso destinazione per rename: parent {} + {}",
                    newparent, newname_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.rename_filesystem_object(&old_path, &new_path) {
            Ok(()) => {
                reply.ok();
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug!("open(ino: {:#x}, flags: {:#x})", ino, flags);

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!(
                    "Impossibile trovare il percorso per inode {:#x} in open",
                    ino
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.open_file(&path, flags) {
            Ok(file_handle) => {
                reply.opened(file_handle, 0);
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        debug!(
            "read(ino: {:#x}, fh: {}, offset: {}, size: {}, flags: {:#x}, lock_owner: {:?})",
            ino, fh, offset, size, flags, lock_owner
        );

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!(
                    "Impossibile trovare il percorso per inode {:#x} in read",
                    ino
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.read_file(&path, fh, offset, size) {
            Ok(data) => {
                reply.data(&data);
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn write(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        debug!(
            "write(ino: {:#x}, fh: {}, offset: {}, data.len(): {}, write_flags: {:#x}, flags: {:#x}, lock_owner: {:?})",
            ino,
            fh,
            offset,
            data.len(),
            write_flags,
            flags,
            lock_owner
        );

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!(
                    "Impossibile trovare il percorso per inode {:#x} in write",
                    ino
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.write_file(&path, fh, offset, data) {
            Ok(bytes_written) => {
                reply.written(bytes_written);
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn release(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        debug!("readdir(ino: {:#x}, fh: {}, offset: {})", ino, fh, offset);

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!(
                    "Impossibile trovare il percorso per inode {:#x} in readdir",
                    ino
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Prima verifica che la directory esista
        match self.get_file_metadata(&path) {
            Some(metadata) if metadata.file_type == fuser::FileType::Directory => {
                // Directory esiste, procedi
            },
            Some(_) => {
                error!("Path {} non è una directory", path);
                reply.error(libc::ENOTDIR);
                return;
            },
            None => {
                error!("Directory {} non trovata", path);
                reply.error(libc::ENOENT);
                return;
            }
        }

        let entries = match self.list_directory(&path) {
            Ok(entries) => entries,
            Err(error_code) => {
                reply.error(error_code);
                return;
            }
        };

        let mut full_entries = vec![
            (".".to_string(), ino, fuser::FileType::Directory),
            ("..".to_string(), 1, fuser::FileType::Directory),
        ];

        info!("Processando {} entries da list_directory", entries.len());

        // Aggiungi tutte le entries trovate
        for (name, entry_ino, file_type) in entries {
            info!("Aggiungendo entry: {} (ino: {}, type: {:?})", name, entry_ino, file_type);
            full_entries.push((name, entry_ino, file_type));
        }

        for (i, (name, entry_ino, file_type)) in
            full_entries.iter().enumerate().skip(offset as usize)
        {
            info!("Sending to FUSE: {} (ino: {}, type: {:?})", name, entry_ino, file_type);
            if reply.add(*entry_ino, (i + 1) as i64, *file_type, name.as_str()) {
                break;
            }
        }

        info!(
            "Readdir completato per {}: {} entries valide",
            path,
            full_entries.len()
        );
        reply.ok();
    }

    fn releasedir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }

    fn access(&mut self, _req: &fuser::Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        debug!("access(ino: {:#x}, mask: {:#o})", ino, mask);

        let path = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!(
                    "Impossibile trovare il percorso per inode {:#x} in access",
                    ino
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Ottieni i metadati del file per controllare i permessi
        match self.get_file_metadata(&path) {
            Some(metadata) => {
                let file_mode = metadata.permissions as i32;
                let uid = _req.uid();
                let gid = _req.gid();

                // Controllo permessi semplificato
                let mut allowed = true;

                // Se l'utente è root, ha sempre accesso
                if uid == 0 {
                    reply.ok();
                    return;
                }

                // Controllo permessi utente/gruppo/altri
                if mask & libc::R_OK != 0 {
                    // Controllo lettura
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o400) != 0; // user read
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o040) != 0; // group read
                    } else {
                        allowed &= (file_mode & 0o004) != 0; // other read
                    }
                }

                if mask & libc::W_OK != 0 {
                    // Controllo scrittura
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o200) != 0; // user write
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o020) != 0; // group write
                    } else {
                        allowed &= (file_mode & 0o002) != 0; // other write
                    }
                }

                if mask & libc::X_OK != 0 {
                    // Controllo esecuzione
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o100) != 0; // user execute
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o010) != 0; // group execute
                    } else {
                        allowed &= (file_mode & 0o001) != 0; // other execute
                    }
                }

                if allowed {
                    debug!("Access granted for {} (mask: {:#o})", path, mask);
                    reply.ok();
                } else {
                    debug!("Access denied for {} (mask: {:#o})", path, mask);
                    reply.error(libc::EACCES);
                }
            }
            None => {
                warn!("File non trovato per access: {}", path);
                reply.error(libc::ENOENT);
            }
        }
    }

    fn create(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        debug!(
            "create(parent: {:#x}, name: {:?}, mode: {:#o}, umask: {:#o}, flags: {:#x})",
            parent, name, mode, umask, flags
        );

        if name.len() > MAX_NAME_LENGTH as usize {
            reply.error(libc::ENAMETOOLONG);
            return;
        }

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file non valido per create: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        let full_path = match self.build_path(parent, name_str) {
            Some(path) => path,
            None => {
                error!(
                    "Impossibile costruire percorso per create: parent {} + {}",
                    parent, name_str
                );
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Crea il file (sempre RegularFile per create)
        let file_type = "RegularFile";
        let effective_flags = flags | libc::O_WRONLY;  // Assicura che il file sia aperto in scrittura
        let effective_mode = (mode & !umask) | libc::S_IFREG;  // Applica umask e forza tipo regular file
        let permissions = effective_mode & !libc::S_IFMT; // Estrai solo i permessi

        info!("Creazione file: {} con permessi {:o}", full_path, permissions);

        // Prima verifica se il file esiste già
        if let Some(_) = self.get_file_metadata(&full_path) {
            error!("File già esistente: {}", full_path);
            reply.error(libc::EEXIST);
            return;
        }

        match self.create_filesystem_object(
            &full_path,
            file_type,
            permissions,
            _req.uid(),
            _req.gid(),
            0,
            umask,
        ) {
            Ok(metadata) => {
                // File creato con successo, ora aprilo
                match self.open_file(&full_path, effective_flags) {
                    Ok(file_handle) => {
                        let file_attr = metadata.to_file_attr();
                        info!(
                            "File created and opened: {} -> fh {} with flags {:#x}",
                            full_path, file_handle, effective_flags
                        );
                        reply.created(
                            &std::time::Duration::from_secs(1),
                            &file_attr,
                            0,
                            file_handle,
                            0,
                        );
                    }
                    Err(error_code) => {
                        error!("Errore apertura file dopo creazione: {}", full_path);
                        // File creato ma non aperto - rimuovilo per consistenza
                        let _ = self.remove_filesystem_object(&full_path, false);
                        reply.error(error_code);
                    }
                }
            }
            Err(error_code) => {
                reply.error(error_code);
            }
        }
    }

    fn flush(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("flush(ino: {:#x}, fh: {})", _ino, _fh);
        reply.ok();
    }

    fn getxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        name: &std::ffi::OsStr,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!("getxattr(ino: {:#x}, name: {:?}, size: {})", _ino, name, size);
        reply.error(libc::ENODATA);
    }

    fn listxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!("listxattr(ino: {:#x}, size: {})", _ino, size);
        if size == 0 {
            reply.size(0);
        } else {
            reply.data(&[]);
        }
    }

    fn setxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        name: &std::ffi::OsStr,
        value: &[u8],
        flags: i32,
        position: u32,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "setxattr(ino: {:#x}, name: {:?}, value_len: {}, flags: {}, position: {})",
            _ino, name, value.len(), flags, position
        );
        reply.error(libc::ENOTSUP);
    }

    fn removexattr(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("removexattr(ino: {:#x}, name: {:?})", _ino, name);
        reply.error(libc::ENODATA);
    }

    fn fsync(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("fsync(ino: {:#x}, fh: {}, datasync: {})", _ino, _fh, _datasync);
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!("fsyncdir(ino: {:#x}, fh: {}, datasync: {})", _ino, _fh, _datasync);
        reply.ok();
    }

    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        debug!("opendir(ino: {:#x}, flags: {:#x})", _ino, _flags);
        // Genera un file handle fittizio per la directory
        let dir_handle = 0;
        reply.opened(dir_handle, 0);
    }
}

} // Fine modulo linux_filesystem

#[cfg(target_os = "linux")]
pub use linux_filesystem::RemoteFsClient;
