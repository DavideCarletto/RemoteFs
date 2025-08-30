#[cfg(target_os = "windows")]
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

#[cfg(target_os = "windows")]
use crate::CacheInvalidationStrategy;
use crate::{client::RemoteFsClient, types::RemoteFsFileType};
use log::{debug, error, info, warn};
use winfsp_wrs::{
    CleanupFlags, CreateFileInfo, CreateOptions, DirInfo, FileAccessRights, FileAttributes,
    FileInfo, FileSystem, FileSystemInterface, NTSTATUS, PSecurityDescriptor, Params,
    STATUS_ACCESS_DENIED, STATUS_DIRECTORY_NOT_EMPTY, STATUS_INVALID_HANDLE,
    STATUS_MEDIA_WRITE_PROTECTED, STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_NOT_FOUND,
    SecurityDescriptor, U16CStr, U16CString, VolumeInfo, VolumeParams, WriteMode, filetime_now,
    u16cstr,
};

#[cfg(target_os = "windows")]
pub struct WinFspRemoteFs {
    client: Mutex<RemoteFsClient>,
    handle_to_path: Arc<Mutex<HashMap<u64, String>>>,
    files_marked_for_deletion: Arc<Mutex<HashSet<u64>>>,
    drive_letter: String,
}

#[cfg(target_os = "windows")]
impl WinFspRemoteFs {
    const FILE_MODE: u32 = 0o644;     
    const DIR_MODE: u32 = 0o755;
    
    fn get_standard_posix_mode(is_directory: bool) -> u32 {
        if is_directory {
            Self::DIR_MODE
        } else {
            Self::FILE_MODE
        }
    }

    fn get_current_file_mode(&self, path: &str) -> Option<u32> {
        let mut client = self.client.lock().unwrap();
        if let Some(metadata) = client.get_file_metadata(path) {
            Some(metadata.permissions as u32)
        } else {
            None
        }
    }

    pub fn new(
        api_url: String,
        drive_letter: String,
        cache_inv_strategy: CacheInvalidationStrategy,
    ) -> Self {
        use std::{
            collections::{HashMap, HashSet},
            sync::{Arc, Mutex},
        };

        info!(
            "Inizializzazione WinFspRemoteFs per API: {} su unità {}",
            api_url, drive_letter
        );

        let mut client = RemoteFsClient::new(api_url);
        client.cache_mut().set_strategy(cache_inv_strategy);

        Self {
            client: Mutex::new(client),
            handle_to_path: Arc::new(Mutex::new(HashMap::new())),
            files_marked_for_deletion: Arc::new(Mutex::new(HashSet::new())),
            drive_letter,
        }
    }

    fn is_windows_system_file(path: &str) -> bool {
        let filename = path.to_lowercase();
        matches!(
            filename.as_str(),
            "/autorun.inf"
                | "\\autorun.inf"
                | "/AutoRun.inf"
                | "\\AutoRun.inf"
                | "/.ds_store"
                | "\\.ds_store"
                | "/thumbs.db"
                | "\\thumbs.db"
                | "/desktop.ini"
                | "\\desktop.ini"
        )
    }

    /// Ottiene un riferimento al client
    pub fn client(&self) -> std::sync::MutexGuard<'_, RemoteFsClient> {
        self.client
            .lock()
            .expect("Failed to lock RemoteFsClient mutex")
    }

    /// Ottiene un riferimento mutabile al client
    pub fn client_mut(&mut self) -> std::sync::MutexGuard<'_, RemoteFsClient> {
        self.client
            .lock()
            .expect("Failed to lock RemoteFsClient mutex")
    }

    /// Ottiene la lettera del drive
    pub fn drive_letter(&self) -> &str {
        &self.drive_letter
    }

    pub fn unmount(&self) -> Result<(), std::io::Error> {
        todo!();
    }

    pub fn mount(self, shutdown_flag: Arc<std::sync::atomic::AtomicBool>) -> Result<(), String> {
        winfsp_wrs::init().expect("Impossibile inizializzare WinFsp");

        let mountpoint = format!("{}:", self.drive_letter.trim_end_matches(':'));
        println!("Montaggio filesystem su {}", mountpoint);

        let mountpoint_u16 = U16CString::from_str(&mountpoint).expect("Mountpoint non valido");
        let mut volume_params = VolumeParams::default();
        volume_params
            .set_sector_size(512)
            .set_sectors_per_allocation_unit(1)
            .set_volume_creation_time(filetime_now())
            .set_volume_serial_number(0)
            .set_file_info_timeout(1000)
            .set_case_sensitive_search(true)
            .set_case_preserved_names(true)
            .set_unicode_on_disk(true)
            .set_persistent_acls(true)
            .set_post_cleanup_when_modified_only(true)
            .set_file_system_name(u16cstr!("remotefs"))
            .unwrap()
            .set_prefix(u16cstr!(""))
            .unwrap();

        let params = Params {
            volume_params,
            ..Default::default()
        };

        let _fs =
            FileSystem::start(params, Some(&mountpoint_u16), self).map_err(|e| e.to_string())?;

        while !shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        info!("Shutdown ricevuto, smontaggio filesystem...");

        Ok(())
    }
}

impl FileSystemInterface for WinFspRemoteFs {
    const CREATE_EX_DEFINED: bool = true;
    fn create_ex(
        &self,
        file_name: &U16CStr,
        create_file_info: CreateFileInfo,
        _security_descriptor: SecurityDescriptor,
        _buffer: &[u8],
        _extra_buffer_is_reparse_point: bool,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        debug!(
            "[WinFSP] create_ex(file_name: {:?}, create_options: {:?})",
            file_name, create_file_info.create_options
        );

        let path = file_name.to_os_string().to_string_lossy().into_owned();
        let is_directory = create_file_info
            .create_options
            .is(CreateOptions::FILE_DIRECTORY_FILE);

        let (metadata, file_handle, normalized_path) = {
            let mut client = self.client.lock().unwrap();
            let normalized_path = client.normalize_path_for_server(&path);
            let posix_mode = Self::get_standard_posix_mode(is_directory);

            let attrs = serde_json::json!({
                "path": normalized_path,
                "file_type": if is_directory { "Directory" } else { "RegularFile" },
                "mode": posix_mode,
                "uid": 1000,
                "gid": 1000,
                "atime": filetime_now(),
                "mtime": filetime_now(),
                "ctime": filetime_now(),
                "crtime": filetime_now(),
            });

            match client.create_filesystem_object(attrs) {
                Ok(metadata) => {
                    let file_handle = if !is_directory {
                        match client.open_file(&path, create_file_info.create_options.0 as i32) {
                            Ok(h) => {
                                info!("File aperto dopo creazione: {} -> handle {}", path, h);
                                h
                            }
                            Err(_) => {
                                error!("Impossibile aprire file appena creato: {}", path);
                                return Err(STATUS_ACCESS_DENIED);
                            }
                        }
                    } else {
                        0
                    };
                    (metadata, file_handle, normalized_path)
                }
                Err(17) => {
                    debug!("File {} già esistente, tentativo di apertura", path);

                    if let Some(existing_metadata) = client.get_file_metadata(&path) {
                        let file_handle = if !is_directory {
                            match client.open_file(&path, create_file_info.create_options.0 as i32)
                            {
                                Ok(h) => {
                                    info!("File esistente aperto: {} -> handle {}", path, h);
                                    h
                                }
                                Err(_) => {
                                    error!("Impossibile aprire file esistente: {}", path);
                                    return Err(STATUS_ACCESS_DENIED);
                                }
                            }
                        } else {
                            0
                        };
                        (existing_metadata, file_handle, normalized_path)
                    } else {
                        error!("File {} risulta esistente ma non trovato", path);
                        return Err(STATUS_OBJECT_NAME_NOT_FOUND);
                    }
                }
                Err(e) => {
                    error!("Errore creazione {}: {}", path, e);
                    return Err(e as NTSTATUS);
                }
            }
        };

        {
            let mut handle_map = self.handle_to_path.lock().unwrap();
            handle_map.insert(file_handle, normalized_path);
        }

        info!(
            "{} creato: {} -> handle {}",
            if is_directory { "Directory" } else { "File" },
            path,
            file_handle
        );

        Ok((file_handle as usize, metadata.to_file_info()))
    }

    const OPEN_DEFINED: bool = true;
    fn open(
        &self,
        file_name: &U16CStr,
        create_options: CreateOptions,
        _granted_access: FileAccessRights,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        debug!(
            "[WinFSP] open(file_name: {:?}, create_options: {:?})",
            file_name, create_options
        );

        let path = file_name.to_os_string().to_string_lossy().into_owned();

        let file_exists = {
            let mut client = self.client.lock().unwrap();
            client.get_file_metadata(&path).is_some()
        };

        if !file_exists {
            let posix_mode = Self::get_standard_posix_mode(false); // Always file for open

            let mut client = self.client.lock().unwrap();
            let normalized_path = client.normalize_path_for_server(&path);

            info!(
                "Auto-creating file with standard POSIX mode: 0o{:o} (decimal: {})",
                posix_mode,
                posix_mode
            );

            let attrs = serde_json::json!({
                "path": normalized_path,
                "file_type": "RegularFile",
                "mode": posix_mode,
                "uid": 1000,
                "gid": 1000,
                "atime": filetime_now(),
                "mtime": filetime_now(),
                "ctime": filetime_now(),
                "crtime": filetime_now(),
            });

            match client.create_filesystem_object(attrs) {
                Ok(metadata) => {
                    info!(
                        "File {} creato automaticamente con inode {}",
                        path, metadata.ino
                    );
                }
                Err(code) => {
                    error!("Impossibile creare file {}: {}", path, code);
                    return Err(code as NTSTATUS);
                }
            }
        }

        let (file_handle, normalized_path) = {
            let mut client = self.client.lock().unwrap();
            let normalized_path = client.normalize_path_for_server(&path);
            match client.open_file(&path, create_options.0 as i32) {
                Ok(handle) => (handle, normalized_path),
                Err(_) => return Err(STATUS_ACCESS_DENIED),
            }
        };

        {
            let mut handle_map = self.handle_to_path.lock().unwrap();
            handle_map.insert(file_handle, normalized_path);
            debug!("Handle {} registrato per path {}", file_handle, path);
        }

        let metadata = {
            let mut client = self.client.lock().unwrap();
            match client.get_file_metadata(&path) {
                Some(meta) => meta,
                None => return Err(STATUS_OBJECT_NAME_NOT_FOUND),
            }
        };

        info!(
            "Apertura completata: path={}, handle={}, è_directory={}",
            path,
            file_handle,
            metadata.file_type == RemoteFsFileType::Directory
        );

        Ok((file_handle as usize, metadata.to_file_info()))
    }

    const OVERWRITE_EX_DEFINED: bool = true;
    fn overwrite_ex(
        &self,
        file_context: Self::FileContext,
        file_attributes: FileAttributes,
        replace_file_attributes: bool,
        allocation_size: u64,
        _buffer: &[u8],
    ) -> Result<FileInfo, NTSTATUS> {
        debug!(
            "[WinFSP] overwrite_ex(file_context: {:?}, file_attributes: {:?}, replace: {}, allocation_size: {})",
            file_context, file_attributes, replace_file_attributes, allocation_size
        );

        let file_handle = file_context as u64;

        let mut updates = serde_json::Map::new();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            handle_map.get(&file_handle).cloned()
        };

        if replace_file_attributes {
            if file_attributes.0 != 0 {
                updates.insert("mode".to_string(), serde_json::json!(file_attributes.0));
            }
        } else {
            if let Some(path) = &path {
                let current_mode = self.get_current_file_mode(path).unwrap_or(Self::get_standard_posix_mode(false));
                
                let updated_mode = if file_attributes.0 == 0 {
                    current_mode | 32 
                } else {
                    current_mode | file_attributes.0 | 32
                };
                
                info!("Preserving file mode during overwrite: {} -> {} (0o{:o})", current_mode, updated_mode, updated_mode);
                updates.insert("mode".to_string(), serde_json::json!(updated_mode));
            } else {
                let fallback_mode = Self::get_standard_posix_mode(false) | 32;
                updates.insert("mode".to_string(), serde_json::json!(fallback_mode));
            }
        }

        updates.insert("size".to_string(), serde_json::json!(allocation_size));

        let updates_json = serde_json::Value::Object(updates);

        let path = {
            let handle_map: std::sync::MutexGuard<'_, HashMap<u64, String>> =
                self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!(
                        "File handle {} non trovato nella mappa in overwrite_ex, tentando di continuare con path vuoto",
                        file_handle
                    );
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let metadata = self
            .client()
            .update_file_metadata(&path, updates_json)
            .ok_or(STATUS_MEDIA_WRITE_PROTECTED)?;

        Ok(metadata.to_file_info())
    }

    const CLEANUP_DEFINED: bool = true;
    fn cleanup(
        &self,
        file_context: Self::FileContext,
        file_name: Option<&U16CStr>,
        flags: CleanupFlags,
    ) {
        let handle = file_context as u64;
        debug!(
            "[WinFSP] cleanup(file_context: {:?}, file_name: {:?}, flags: {:?})",
            file_context, file_name, flags
        );

        let should_delete = {
            let marked_files = self.files_marked_for_deletion.lock().unwrap();
            marked_files.contains(&handle)
        };

        if should_delete {
            let path = {
                let handle_map = self.handle_to_path.lock().unwrap();
                handle_map.get(&handle).cloned()
            };

            if let Some(path) = path {
                let mut client = self.client.lock().unwrap();
                let is_directory = match client.get_file_metadata(&path) {
                    Some(meta) => meta.file_type == RemoteFsFileType::Directory,
                    None => {
                        warn!("File {} non trovato durante cleanup", path);
                        false
                    }
                };

                match client.remove_filesystem_object(&path, is_directory) {
                    Ok(()) => {
                        info!("File eliminato con successo durante cleanup: {}", path);
                        let mut marked_files = self.files_marked_for_deletion.lock().unwrap();
                        marked_files.remove(&handle);
                    }
                    Err(error_code) => {
                        error!(
                            "Errore eliminazione in cleanup {}: {} (code: {})",
                            path, error_code, error_code
                        );
                    }
                }
            } else {
                error!("Handle {} non trovato nella mappa durante cleanup", handle);
                let mut marked_files = self.files_marked_for_deletion.lock().unwrap();
                marked_files.remove(&handle);
            }
        }

        info!(
            "Cleanup completato per handle {} (file eliminato: {})",
            handle, should_delete
        );
    }

    const READ_DEFINED: bool = true;
    fn read(
        &self,
        file_context: Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<usize, NTSTATUS> {
        let file_handle = file_context as u64;

        debug!(
            "[WinFSP] read(file_context: {:?}, buffer_size: {}, offset: {})",
            file_context,
            buffer.len(),
            offset
        );

        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => {
                    debug!("Handle {} trovato per lettura: {}", file_handle, p);
                    p.clone()
                }
                None => {
                    debug!(
                        "Handle {} non trovato nella mappa per lettura - potrebbe essere stato chiuso",
                        file_handle
                    );

                    let handle_map_ref = &*handle_map;
                    if let Some((valid_handle, valid_path)) = handle_map_ref.iter().next() {
                        warn!(
                            "Handle chiuso {}: usando fallback {} -> {}",
                            file_handle, valid_handle, valid_path
                        );
                        valid_path.clone()
                    } else {
                        error!("Nessun handle valido disponibile per lettura");
                        return Err(STATUS_INVALID_HANDLE);
                    }
                }
            }
        };

        let file_size = match client.get_file_metadata(&path) {
            Some(meta) => {
                debug!("File size per {}: {} bytes", path, meta.size);
                meta.size
            }
            None => {
                error!("Impossibile ottenere metadati per file {}", path);
                return Err(STATUS_OBJECT_NAME_NOT_FOUND);
            }
        };

        if offset >= file_size {
            debug!(
                "Offset {} >= file_size {}, ritorno 0 bytes",
                offset, file_size
            );
            if offset > file_size && offset > 65536 {
                warn!(
                    "Offset troppo grande {} per file {} bytes. App potrebbe essere in loop!",
                    offset, file_size
                );
            }
            return Ok(0);
        }

        let remaining_bytes = file_size - offset;
        let bytes_to_read = std::cmp::min(buffer.len() as u64, remaining_bytes) as u32;

        debug!(
            "Lettura: path={}, offset={}, buffer_len={}, file_size={}, bytes_to_read={}",
            path,
            offset,
            buffer.len(),
            file_size,
            bytes_to_read
        );

        let valid_handle = {
            let handle_map = self.handle_to_path.lock().unwrap();
            handle_map
                .iter()
                .find(|(_, p)| **p == path)
                .map(|(h, _)| *h)
                .unwrap_or_else(|| handle_map.keys().next().copied().unwrap_or(file_handle))
        };

        match client.read_file(&path, valid_handle, offset as i64, bytes_to_read) {
            Ok(data) => {
                let bytes_read = data.len();

                if bytes_read == 0 && bytes_to_read > 0 {
                    warn!(
                        "Server ha restituito 0 bytes ma ne erano richiesti {}",
                        bytes_to_read
                    );
                    return Ok(0);
                }

                if bytes_read > buffer.len() {
                    error!(
                        "Server ha restituito più dati ({}) del buffer disponibile ({})",
                        bytes_read,
                        buffer.len()
                    );
                    return Err(STATUS_ACCESS_DENIED);
                }

                buffer[..bytes_read].copy_from_slice(&data[..bytes_read]);

                info!("Lettura completata: {} -> {} bytes", path, bytes_read);
                Ok(bytes_read)
            }
            Err(error_code) => {
                error!("Errore lettura {}: {}", path, error_code);
                Err(STATUS_ACCESS_DENIED)
            }
        }
    }

    const WRITE_DEFINED: bool = true;
    fn write(
        &self,
        file_context: Self::FileContext,
        buffer: &[u8],
        mode: WriteMode,
    ) -> Result<(usize, FileInfo), NTSTATUS> {
        debug!(
            "[WinFSP] write(file_context: {:?}, buffer.len(): {}, mode: {:?})",
            file_context,
            buffer.len(),
            mode
        );

        let file_handle = file_context as u64;

        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let offset = match mode {
            WriteMode::Normal { offset } => {
                debug!("WriteMode::Normal con offset: {}", offset);
                offset as i64
            }
            WriteMode::ConstrainedIO { offset } => {
                debug!("WriteMode::ConstrainedIO con offset: {}", offset);
                offset as i64
            }
            WriteMode::WriteToEOF => {
                let eof_offset = match client.get_file_metadata(&path) {
                    Some(meta) => {
                        debug!(
                            "WriteMode::WriteToEOF - dimensione file attuale: {}",
                            meta.size
                        );
                        meta.size as i64
                    }
                    None => {
                        debug!("WriteMode::WriteToEOF - file non trovato, offset = 0");
                        0
                    }
                };
                eof_offset
            }
        };

        match client.write_file(&path, file_handle, offset, buffer) {
            Ok(written) => {
                info!("File scritto: {} -> {} bytes", path, written);

                let metadata = client
                    .get_file_metadata(&path)
                    .map(|m| m.to_file_info())
                    .unwrap_or_else(FileInfo::default);
                Ok((written as usize, metadata))
            }
            Err(error_code) => {
                error!(
                    "Errore durante la scrittura del file {}: {}",
                    path, error_code
                );
                Err(STATUS_ACCESS_DENIED)
            }
        }
    }

    const FLUSH_DEFINED: bool = true;

    fn flush(&self, file_context: Self::FileContext) -> Result<FileInfo, i32> {
        debug!("[WinFSP] flush(file_context: {:?})", file_context);

        let file_handle = file_context as u64;

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!(
                        "File handle {} non trovato nella mappa per flush",
                        file_handle
                    );
                    return Err(STATUS_INVALID_HANDLE as i32);
                }
            }
        };

        let mut client = self.client.lock().unwrap();
        match client.flush_file(&path, file_handle) {
            Ok(()) => {
                info!("Flush completato per handle {} ({})", file_context, path);

                match client.get_file_metadata(&path) {
                    Some(metadata) => Ok(metadata.to_file_info()),
                    None => {
                        error!("Impossibile ottenere metadati dopo flush per {}", path);
                        Ok(FileInfo::default())
                    }
                }
            }
            Err(error_code) => {
                error!(
                    "Errore durante flush per {} (handle {}): {}",
                    path, file_handle, error_code
                );
                Err(error_code as i32)
            }
        }
    }

    const GET_FILE_INFO_DEFINED: bool = true;
    fn get_file_info(&self, file_context: Self::FileContext) -> Result<FileInfo, NTSTATUS> {
        debug!("[WinFSP] get_file_info(file_context: {:?})", file_context);

        let file_handle = file_context as u64;

        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    debug!(
                        "File handle {} non trovato nella mappa, assumo root directory",
                        file_handle
                    );
                    "/".to_string()
                }
            }
        };

        match client.get_file_metadata(&path) {
            Some(meta) => Ok(meta.to_file_info()),
            None => {
                error!("Metadati non trovati per file {}", path);
                Err(STATUS_OBJECT_NAME_NOT_FOUND)
            }
        }
    }

    const SET_BASIC_INFO_DEFINED: bool = true;
    fn set_basic_info(
        &self,
        file_context: Self::FileContext,
        file_attributes: FileAttributes,
        creation_time: u64,
        last_access_time: u64,
        last_write_time: u64,
        change_time: u64,
    ) -> Result<FileInfo, NTSTATUS> {
        debug!(
            "[WinFSP] set_basic_info(file_context: {:?}, file_attributes: {:?}, creation_time: {:?}, last_access_time: {:?}, last_write_time: {:?}, change_time: {:?})",
            file_context,
            file_attributes,
            creation_time,
            last_access_time,
            last_write_time,
            change_time
        );

        let file_handle = file_context as u64;

        if file_handle == 0 {
            debug!("set_basic_info: handle 0 root directory, ignorato");
            return Ok(FileInfo::default());
        }

        if creation_time == 0 && last_access_time == 0 && last_write_time == 0 && change_time == 0 {
            let path = {
                let handle_map = self.handle_to_path.lock().unwrap();
                match handle_map.get(&file_handle) {
                    Some(p) => p.clone(),
                    None => {
                        error!("File handle {} non trovato nella mappa", file_handle);
                        return Err(STATUS_INVALID_HANDLE);
                    }
                }
            };

            let mut client = self.client.lock().unwrap();
            match client.get_file_metadata(&path) {
                Some(metadata) => {
                    debug!("Restituiti metadata esistenti per: {}", path);
                    return Ok(metadata.to_file_info());
                }
                None => {
                    error!("File {} non trovato per set_basic_info", path);
                    return Err(STATUS_OBJECT_NAME_NOT_FOUND);
                }
            }
        }

        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let mut updates = serde_json::Map::new();

        if file_attributes.0 != 0xFFFFFFFF && file_attributes.0 != 0x80000 {
            info!(
                "File attributes change for {}: 0x{:x} (decimal: {})", 
                path, file_attributes.0, file_attributes.0
            );
            
            if file_attributes.0 & 0x20 != 0 {
                debug!("  - ARCHIVE bit set (file was modified)");
            }
            if file_attributes.0 & 0x01 != 0 {
                debug!("  - READONLY bit set");
            }
            if file_attributes.0 & 0x02 != 0 {
                debug!("  - HIDDEN bit set");
            }
            if file_attributes.0 & 0x04 != 0 {
                debug!("  - SYSTEM bit set");
            }
            
            let final_mode = if file_attributes.0 == 32 {
                match self.get_current_file_mode(&path) {
                    Some(current_mode) => {
                        let base_permissions = current_mode & 0o777; 
                        let result = base_permissions | 32;
                        info!("Preserving POSIX permissions 0o{:o} and adding ARCHIVE bit -> {}", 
                              base_permissions, result);
                        result
                    }
                    None => {
                        info!("No existing permissions found, using standard file mode with ARCHIVE");
                        Self::get_standard_posix_mode(false) | 32
                    }
                }
            } else {
                file_attributes.0
            };
            
            updates.insert("mode".to_string(), serde_json::json!(final_mode));
        }

        if creation_time != 0 {
            updates.insert("crtime".to_string(), serde_json::json!(creation_time));
        }
        if last_access_time != 0 {
            updates.insert("atime".to_string(), serde_json::json!(last_access_time));
        }
        if last_write_time != 0 {
            updates.insert("mtime".to_string(), serde_json::json!(last_write_time));
        }
        if change_time != 0 {
            updates.insert("ctime".to_string(), serde_json::json!(change_time));
        }

        if updates.is_empty() {
            match client.get_file_metadata(&path) {
                Some(meta) => return Ok(meta.to_file_info()),
                None => return Err(STATUS_OBJECT_NAME_NOT_FOUND),
            }
        }

        match client.update_file_metadata(&path, serde_json::Value::Object(updates)) {
            Some(meta) => Ok(meta.to_file_info()),
            None => {
                error!("Impossibile aggiornare attributi per {}", path);
                Err(STATUS_ACCESS_DENIED)
            }
        }
    }

    const SET_FILE_SIZE_DEFINED: bool = true;
    fn set_file_size(
        &self,
        file_context: Self::FileContext,
        new_size: u64,
        allocation_size: bool,
    ) -> Result<FileInfo, NTSTATUS> {
        debug!(
            "[WinFSP] set_file_size(file_context: {:?}, new_size: {}, allocation_size: {})",
            file_context, new_size, allocation_size
        );

        let file_handle = file_context as u64;
        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let updates = serde_json::json!({
            "size": new_size
        });

        match self.client().update_file_metadata(&path, updates) {
            Some(metadata) => {
                info!("Dimensione file aggiornata: {} -> {} bytes", path, new_size);
                Ok(metadata.to_file_info())
            }
            None => {
                error!("Impossibile aggiornare dimensione per {}", path);
                Err(STATUS_MEDIA_WRITE_PROTECTED)
            }
        }
    }

    const RENAME_DEFINED: bool = true;
    fn rename(
        &self,
        file_context: Self::FileContext,
        file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> Result<(), NTSTATUS> {
        debug!(
            "[WinFSP] rename(file_context: {:?}, file_name: {:?}, new_file_name: {:?}, replace_if_exists: {:?})",
            file_context, file_name, new_file_name, replace_if_exists
        );

        let file_handle = file_context as u64;

        let old_path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa per rename", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let new_path = new_file_name.to_os_string().to_string_lossy().into_owned();

        {
            let mut client = self.client.lock().unwrap();
            if !replace_if_exists {
                if client.get_file_metadata(&new_path).is_some() {
                    error!(
                        "File destinazione esistente e replace_if_exists=false: {}",
                        new_path
                    );
                    return Err(STATUS_ACCESS_DENIED);
                }
            }

            match client.rename_filesystem_object(&old_path, &new_path) {
                Ok(()) => {
                    info!("Rinomina riuscita: {} -> {}", old_path, new_path);
                    
                    {
                        let mut handle_map = self.handle_to_path.lock().unwrap();
                        if handle_map.contains_key(&file_handle) {
                            let new_normalized_path = client.normalize_path_for_server(&new_path);
                            handle_map.insert(file_handle, new_normalized_path);
                            debug!("Handle {} aggiornato con nuovo path: {}", file_handle, new_path);
                        }
                    }
                    
                    Ok(())
                }
                Err(code) => {
                    error!(
                        "Errore backend durante rinomina: {} -> {} code: {}",
                        old_path, new_path, code
                    );
                    Err(code as NTSTATUS)
                }
            }
        }
    }

    const GET_SECURITY_DEFINED: bool = true;
    fn get_security(
        &self,
        file_context: Self::FileContext,
    ) -> Result<PSecurityDescriptor, NTSTATUS> {
        debug!("[WinFSP] get_security(file_context: {:?})", file_context);

        let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
            "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
        ))
        .map_err(|_| STATUS_ACCESS_DENIED)?;

        Ok(security_descriptor.as_ptr())
    }

    const SET_SECURITY_DEFINED: bool = true;
    fn set_security(
        &self,
        file_context: Self::FileContext,
        security_information: u32,
        _modification_descriptor: PSecurityDescriptor,
    ) -> Result<(), NTSTATUS> {
        let file_handle = file_context as u64;
        debug!(
            "[WinFSP] set_security(file_context: {:?}, security_information: 0x{:x})", 
            file_context, security_information
        );

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa per set_security", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let new_mode = match security_information {
            0x4 => {
                let is_directory = {
                    let mut client = self.client.lock().unwrap();
                    match client.get_file_metadata(&path) {
                        Some(meta) => meta.file_type == RemoteFsFileType::Directory,
                        None => false,
                    }
                };
                
                if is_directory {
                    0o755
                } else {
                    0o644
                }
            }
            0x1 => {
                info!("Owner change detected");
                Self::get_standard_posix_mode(false)
            }
            0x2 => {
                info!("Group change detected");
                Self::get_standard_posix_mode(false) 
            }
            _ => {
                info!("Other security change detected: 0x{:x}", security_information);
                Self::get_standard_posix_mode(false)
            }
        };

        let mut updates = serde_json::Map::new();
        updates.insert("mode".to_string(), serde_json::json!(new_mode));

        {
            let mut client = self.client.lock().unwrap();
            match client.update_file_metadata(&path, serde_json::Value::Object(updates)) {
                Some(_) => {
                    info!("Security attributes updated for {}", path);
                    Ok(())
                }
                None => {
                    error!("Failed to update security for {}", path);
                    Err(STATUS_ACCESS_DENIED)
                }
            }
        }
    }

    const READ_DIRECTORY_DEFINED: bool = true;
    fn read_directory(
        &self,
        file_context: Self::FileContext,
        marker: Option<&U16CStr>,
        mut add_dir_info: impl FnMut(DirInfo) -> bool,
    ) -> Result<(), NTSTATUS> {
        debug!(
            "[WinFSP] read_directory(file_context: {:?}, marker: {:?})",
            file_context, marker
        );

        let file_handle = file_context as u64;
        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        match client.get_file_metadata(&path) {
            Some(metadata) if metadata.file_type == crate::types::RemoteFsFileType::Directory => {}
            Some(_) => {
                error!("Path {} non è una directory", path);
                return Err(STATUS_NOT_A_DIRECTORY);
            }
            None => {
                error!("Directory {} non trovata", path);
                return Err(STATUS_OBJECT_NAME_NOT_FOUND);
            }
        }

        let entries = match client.list_directory(&path) {
            Ok(entries) => entries,
            Err(_) => return Err(STATUS_ACCESS_DENIED),
        };
        let entries_len = entries.len();

        let marker_str = marker.map(|m| m.to_string_lossy().to_string());
        let mut should_continue = true;

        for (name, _ino, _file_type) in entries {
            if !should_continue {
                break;
            }

            if let Some(ref marker_name) = marker_str {
                if name <= *marker_name {
                    continue;
                }
            }

            let full_path = if path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", path, name)
            };

            let metadata = match client.get_file_metadata(&full_path) {
                Some(meta) => meta,
                None => continue,
            };

            let dir_info = DirInfo::from_str(metadata.to_file_info(), &name);
            should_continue = add_dir_info(dir_info);
        }

        info!(
            "Read directory completato per {}: {} entries processate",
            path, entries_len
        );
        Ok(())
    }

    const SET_DELETE_DEFINED: bool = true;
    fn set_delete(
        &self,
        file_context: Self::FileContext,
        file_name: &U16CStr,
        delete_file: bool,
    ) -> Result<(), NTSTATUS> {
        debug!(
            "[WinFSP] set_delete(file_context: {:?}, file_name: {:?}, delete: {})",
            file_context, file_name, delete_file
        );

        let file_name_str = file_name.to_string_lossy();
        let handle = file_context as u64;
        {
            let handle_map = self.handle_to_path.lock().unwrap();
            if let Some(path) = handle_map.get(&handle) {
                let mut client = self.client.lock().unwrap();
                let _exists = client.get_file_metadata(path).is_some();
            } else {
                warn!("Handle {} non trovato nella mappa!", handle);
            }
        };

        if file_name_str == "." || file_name_str == ".." {
            info!("Ignorato set_delete per entry speciale: {}", file_name_str);
            return Ok(());
        }

        let mut marked_files = self.files_marked_for_deletion.lock().unwrap();
        if delete_file {
            marked_files.insert(handle);
            info!(
                "File marcato per eliminazione: {} (handle: {})",
                file_name_str, handle
            );
        } else {
            marked_files.remove(&handle);
            info!(
                "Marcatura eliminazione rimossa per: {} (handle: {})",
                file_name_str, handle
            );
        }

        Ok(())
    }

    const GET_VOLUME_INFO_DEFINED: bool = false;

    const SET_VOLUME_LABEL_DEFINED: bool = false;

    const CREATE_DEFINED: bool = true;

    const OVERWRITE_DEFINED: bool = false;

    const CLOSE_DEFINED: bool = true;
    fn close(&self, file_context: Self::FileContext) -> () {
        let file_handle = file_context as u64;
        debug!("[WinFSP] close(file_context: {})", file_handle);

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            handle_map.get(&file_handle).cloned()
        };

        if let Some(ref path) = path {
            let mut client = self.client.lock().unwrap();
            if let Some(metadata) = client.get_file_metadata(path) {
                if metadata.file_type == crate::types::RemoteFsFileType::RegularFile
                    && metadata.size > 4096
                {
                    match client.flush_file(path, file_handle) {
                        Ok(_) => debug!("File flushed successfully: {}", path),
                        Err(e) => warn!("Failed to flush file {}: {}", path, e),
                    }
                }
            }
        }

        if let Some(path) = path {
            debug!("File chiuso ma handle mantenuto temporaneamente: {} (handle {})", path, file_handle);
        } else {
            warn!("Handle {} chiuso ma non trovato nella mappa", file_handle);
        }
    }

    const CAN_DELETE_DEFINED: bool = true;
    fn can_delete(
        &self,
        file_context: Self::FileContext,
        file_name: &U16CStr,
    ) -> Result<(), NTSTATUS> {
        debug!(
            "[WinFSP] can_delete(file_context: {:?}, file_name: {:?})",
            file_context, file_name
        );

        let file_handle = file_context as u64;

        let _file_name_str = file_name.to_string_lossy();

        let mut client = self.client.lock().unwrap();
        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => {
                    info!("Path trovato per handle {}: {}", file_handle, p);
                    p.clone()
                }
                None => {
                    error!(
                        "Handle {} non trovato nella mappa per can_delete",
                        file_handle
                    );
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        match client.get_file_metadata(&path) {
            Some(meta) => {
                info!(
                    "Metadata per {}: tipo={:?}, size={}",
                    path, meta.file_type, meta.size
                );
                if meta.file_type == RemoteFsFileType::Directory {
                    match client.list_directory(&path) {
                        Ok(entries) => {
                            let has_real_files =
                                entries.iter().any(|(name, ..)| name != "." && name != "..");
                            info!(
                                "Directory {}: {} entries totali, ha file reali: {}",
                                path,
                                entries.len(),
                                has_real_files
                            );

                            if has_real_files {
                                warn!("Directory {} non vuota, eliminazione negata", path);
                                Err(STATUS_DIRECTORY_NOT_EMPTY)
                            } else {
                                info!("Directory {} vuota, eliminazione consentita", path);
                                Ok(())
                            }
                        }
                        Err(e) => {
                            error!("Errore listing directory {}: {}", path, e);
                            Err(STATUS_ACCESS_DENIED)
                        }
                    }
                } else {
                    info!("File regolare {}, eliminazione consentita", path);
                    Ok(())
                }
            }
            None => {
                error!("File {} non trovato per can_delete", path);
                Err(STATUS_OBJECT_NAME_NOT_FOUND)
            }
        }
    }

    const GET_REPARSE_POINT_DEFINED: bool = false;

    const SET_REPARSE_POINT_DEFINED: bool = false;

    const DELETE_REPARSE_POINT_DEFINED: bool = false;

    const GET_STREAM_INFO_DEFINED: bool = false;

    const GET_DIR_INFO_BY_NAME_DEFINED: bool = false;

    const CONTROL_DEFINED: bool = false;

    const GET_EA_DEFINED: bool = false;

    const SET_EA_DEFINED: bool = false;

    const DISPATCHER_STOPPED_DEFINED: bool = true;

    const RESOLVE_REPARSE_POINTS_DEFINED: bool = false;

    const GET_SECURITY_BY_NAME_DEFINED: bool = true;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _find_reparse_point: impl Fn() -> Option<FileAttributes>,
    ) -> Result<(FileAttributes, PSecurityDescriptor, bool), NTSTATUS> {
        debug!("[WinFSP] get_security_by_name(file_name: {:?})", file_name);

        let path = file_name.to_os_string().to_string_lossy().into_owned();

        if Self::is_windows_system_file(&path) {
            debug!("Ignorato file di sistema Windows: {}", path);
            return Err(STATUS_OBJECT_NAME_NOT_FOUND);
        }

        if path == "." || path == ".." {
            let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
                "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
            ))
            .map_err(|_| STATUS_ACCESS_DENIED)?;
            return Ok((
                FileAttributes::DIRECTORY,
                security_descriptor.as_ptr(),
                false,
            ));
        }

        let mut client = self.client.lock().unwrap();

        if let Some(metadata) = client.get_file_metadata(&path) {
            let posix_mode = metadata.permissions as u32;

            let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
                "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
            ))
            .map_err(|_| STATUS_ACCESS_DENIED)?;

            let file_attributes = if metadata.file_type == RemoteFsFileType::Directory {
                FileAttributes::DIRECTORY
            } else {
                FileAttributes(posix_mode)
            };

            Ok((file_attributes, security_descriptor.as_ptr(), false))
        } else {
            debug!("File {} non esiste per get_security_by_name (normale per Windows)", path);
            Err(STATUS_OBJECT_NAME_NOT_FOUND)
        }
    }

    fn get_volume_info(&self) -> Result<VolumeInfo, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn set_volume_label(&self, _volume_label: &U16CStr) -> Result<VolumeInfo, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn create(
        &self,
        file_name: &U16CStr,
        create_file_info: CreateFileInfo,
        _security_descriptor: SecurityDescriptor,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        debug!(
            "[WinFSP] create(file_name: {:?}, create_options: {:?})",
            file_name, create_file_info.create_options
        );

        let path = file_name.to_os_string().to_string_lossy().into_owned();
        let is_directory = create_file_info
            .create_options
            .is(CreateOptions::FILE_DIRECTORY_FILE);

        let mut client = self.client.lock().unwrap();
        let normalized_path = client.normalize_path_for_server(&path);
        let posix_mode = Self::get_standard_posix_mode(is_directory);

        let attrs = serde_json::json!({
            "path": normalized_path,
            "file_type": if is_directory { "Directory" } else { "RegularFile" },
            "mode": posix_mode,
            "uid": 1000,
            "gid": 1000,
            "atime": filetime_now(),
            "mtime": filetime_now(),
            "ctime": filetime_now(),
            "crtime": filetime_now(),
        });

        match client.create_filesystem_object(attrs) {
            Ok(metadata) => {
                let file_handle = if !is_directory {
                    match client.open_file(&path, create_file_info.create_options.0 as i32) {
                        Ok(h) => {
                            info!("File creato e aperto: {} -> handle {}", path, h);
                            h
                        }
                        Err(_) => {
                            error!("Impossibile aprire file appena creato: {}", path);
                            return Err(STATUS_ACCESS_DENIED);
                        }
                    }
                } else {
                    info!("Directory creata: {} -> ino {}", path, metadata.ino);
                    metadata.ino as u64
                };

                {
                    let mut handle_map = self.handle_to_path.lock().unwrap();
                    handle_map.insert(file_handle, path.clone());
                    debug!("Handle {} registrato per path {}", file_handle, path);
                }

                info!("Creazione completata: {} -> handle {}", path, file_handle);
                Ok((file_handle as usize, metadata.to_file_info()))
            }
            Err(error_code) => {
                error!("Errore creazione {}: code {}", path, error_code);
                Err(STATUS_ACCESS_DENIED)
            }
        }
    }

    fn overwrite(
        &self,
        _file_context: Self::FileContext,
        _file_attributes: FileAttributes,
        _replace_file_attributes: bool,
        _allocation_size: u64,
    ) -> Result<FileInfo, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn get_reparse_point(
        &self,
        _file_context: Self::FileContext,
        _file_name: &U16CStr,
        _buffer: &mut [u8],
    ) -> Result<usize, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn set_reparse_point(
        &self,
        _file_context: Self::FileContext,
        _file_name: &U16CStr,
        _buffer: &mut [u8],
    ) -> Result<(), NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn delete_reparse_point(
        &self,
        _file_context: Self::FileContext,
        _file_name: &U16CStr,
        _buffer: &mut [u8],
    ) -> Result<(), NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn get_stream_info(
        &self,
        _file_context: Self::FileContext,
        _buffer: &mut [u8],
    ) -> Result<usize, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn get_dir_info_by_name(
        &self,
        _file_context: Self::FileContext,
        _file_name: &U16CStr,
    ) -> Result<FileInfo, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn control(
        &self,
        _file_context: Self::FileContext,
        _control_code: u32,
        _input_buffer: &[u8],
        _output_buffer: &mut [u8],
    ) -> Result<usize, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn get_ea(&self, _file_context: Self::FileContext, _buffer: &[u8]) -> Result<usize, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn set_ea(
        &self,
        _file_context: Self::FileContext,
        _buffer: &[u8],
    ) -> Result<FileInfo, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    fn dispatcher_stopped(&self, normally: bool) {
        info!("=== CACHE STATISTICS ===");
        info!("{}", self.client().cache().get_stats());
        
        if normally {
            info!("Filesystem Windows smontato normalmente");
        } else {
            warn!("Filesystem Windows smontato in modo anomalo");
        }
    }

    fn get_reparse_point_by_name(
        &self,
        _file_name: &U16CStr,
        _is_directory: bool,
        _buffer: Option<&mut [u8]>,
    ) -> Result<usize, NTSTATUS> {
        std::unreachable!("To be used, trait method must be overwritten !");
    }

    type FileContext = usize;
}
