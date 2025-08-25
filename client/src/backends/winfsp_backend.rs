#[cfg(target_os = "windows")]
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{client::RemoteFsClient, types::RemoteFsFileType};
use log::{error, info};
use winfsp_wrs::{
    CleanupFlags, CreateFileInfo, CreateOptions, DirInfo, FileAccessRights, FileAttributes,
    FileInfo, FileSystem, FileSystemInterface, NTSTATUS, PSecurityDescriptor, Params,
    STATUS_ACCESS_DENIED, STATUS_DIRECTORY_NOT_EMPTY, STATUS_INVALID_HANDLE,
    STATUS_MEDIA_WRITE_PROTECTED, STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_NOT_FOUND,
    SecurityDescriptor, U16CStr, U16CString, VolumeInfo, VolumeParams, WriteMode, filetime_now,
    u16cstr,
};

macro_rules! debug {
    (target: $target:expr, $($arg:tt)+) => { println!($target, $($arg)+) };
    ($($arg:tt)+) => { println!($($arg)+) };
}

/// Implementazione WinFSP del filesystem remoto
#[cfg(target_os = "windows")]
pub struct WinFspRemoteFs {
    client: Mutex<RemoteFsClient>,
    handle_to_path: Arc<Mutex<HashMap<u64, String>>>,
    files_marked_for_deletion: Arc<Mutex<HashSet<u64>>>,
    drive_letter: String,
}

#[cfg(target_os = "windows")]
impl WinFspRemoteFs {
    pub fn new(api_url: String, drive_letter: String) -> Self {
        use std::{
            collections::{HashMap, HashSet},
            sync::{Arc, Mutex},
        };

        info!(
            "Inizializzazione WinFspRemoteFs per API: {} su unità {}",
            api_url, drive_letter
        );

        let client = RemoteFsClient::new(api_url);

        Self {
            client: Mutex::new(client),
            handle_to_path: Arc::new(Mutex::new(HashMap::new())),
            files_marked_for_deletion: Arc::new(Mutex::new(HashSet::new())),
            drive_letter,
        }
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

    pub fn mount(self) -> Result<(), String> {
        // Cambia da &mut self a self
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

        // Ora passa self (ownership completo)
        let fs =
            FileSystem::start(params, Some(&mountpoint_u16), self).map_err(|e| e.to_string())?;

        println!(
            "Filesystem montato su {}! Premi INVIO per smontare...",
            mountpoint
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        println!("Smontaggio filesystem...");
        fs.stop();

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

        let (metadata, file_handle) = {
            let mut client = self.client.lock().unwrap();
            let normalized_path = client.normalize_path_for_server(&path);

            let attrs = serde_json::json!({
                "path": normalized_path,
                "file_type": if is_directory { "Directory" } else { "RegularFile" },
                "mode": create_file_info.file_attributes.0,
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
                    (metadata, file_handle)
                }
                Err(17) => {
                    // File già esistente - proviamo ad aprirlo invece di crearlo
                    debug!("File {} già esistente, tentativo di apertura", path);
                    
                    if let Some(existing_metadata) = client.get_file_metadata(&path) {
                        let file_handle = if !is_directory {
                            match client.open_file(&path, create_file_info.create_options.0 as i32) {
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
                        (existing_metadata, file_handle)
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
            let normalized_path = self.client().normalize_path_for_server(&path); // ← Nuovo lock breve
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

        let file_handle = match self.client().open_file(&path, create_options.0 as i32) {
            Ok(handle) => handle,
            Err(_) => return Err(STATUS_ACCESS_DENIED),
        };

        // Sempre registra l'handle nella mappa, anche se è duplicato
        {
            let mut handle_map = self.handle_to_path.lock().unwrap();
            handle_map.insert(file_handle, path.clone());
            debug!("Handle {} registrato per path {}", file_handle, path);
        }

        let metadata = match self.client().get_file_metadata(&path) {
            Some(meta) => meta,
            None => return Err(STATUS_OBJECT_NAME_NOT_FOUND),
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

        if replace_file_attributes {
            updates.insert("mode".to_string(), serde_json::json!(file_attributes.0));
        } else {
            let updated_mode = file_attributes.0 | FileAttributes::ARCHIVE.0;
            updates.insert("mode".to_string(), serde_json::json!(updated_mode));
        }

        updates.insert("size".to_string(), serde_json::json!(allocation_size));

        let updates_json = serde_json::Value::Object(updates);

        let path = {
            let handle_map: std::sync::MutexGuard<'_, HashMap<u64, String>> =
                self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    // Se l'handle non è nella mappa, restituiamo un errore per ora
                    // TODO: implementare un fallback più sofisticato
                    error!("File handle {} non trovato nella mappa in overwrite_ex, tentando di continuare con path vuoto", file_handle);
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
        debug!(
            "[WinFSP] cleanup(file_context: {:?}, file_name: {:?}, flags: {:?})",
            file_context, file_name, flags
        );

        let handle = file_context as u64;

        // Controlla se questo file è stato marcato per eliminazione
        let mut marked_files = self.files_marked_for_deletion.lock().unwrap();
        let should_delete = marked_files.contains(&handle);
        
        if should_delete {
            // Rimuovi dalla lista dei file marcati
            marked_files.remove(&handle);
            drop(marked_files); // Rilascia il lock prima delle operazioni
            
            let handle_map = self.handle_to_path.lock().unwrap();
            if let Some(path) = handle_map.get(&handle) {
                let path_clone = path.clone();
                drop(handle_map); // Rilascia il lock prima della chiamata al client

                let mut client = self.client.lock().unwrap();
                let is_directory = match client.get_file_metadata(&path_clone) {
                    Some(meta) => meta.file_type == RemoteFsFileType::Directory,
                    None => false,
                };

                info!("Tentativo eliminazione file in cleanup: {} (handle: {})", path_clone, handle);
                match client.remove_filesystem_object(&path_clone, is_directory) {
                    Ok(()) => {
                        info!("File/Directory eliminato in cleanup: {}", path_clone);
                        
                        // Rimuovi l'handle dalla mappa solo dopo eliminazione riuscita
                        let mut handle_map = self.handle_to_path.lock().unwrap();
                        handle_map.remove(&handle);
                    }
                    Err(error_code) => {
                        error!("Errore eliminazione in cleanup {}: {} (code: {})", path_clone, error_code, error_code);
                    }
                }
            }
        } else {
            drop(marked_files); // Rilascia il lock se non serve eliminare
        }

        
        debug!("Cleanup completato per handle {} (file eliminato: {})", handle, should_delete);
    }

    const READ_DEFINED: bool = true;
    fn read(
        &self,
        file_context: Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> Result<usize, NTSTATUS> {
        debug!(
            "[WinFSP] read(file_context: {:?}, buffer_size: {}, offset: {})",
            file_context,
            buffer.len(),
            offset
        );

        let file_handle = file_context as u64;

        let mut client = self.client.lock().unwrap();

        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => {
                    debug!("Handle {} trovato per lettura: {}", file_handle, p);
                    p.clone()
                }
                None => {
                    error!("File handle {} non trovato nella mappa per lettura", file_handle);
                    debug!("Handle attualmente registrati: {:?}", handle_map.keys().collect::<Vec<_>>());
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        match client.read_file(&path, file_handle, offset as i64, buffer.len() as u32) {
            Ok(data) => {
                let to_copy = data.len().min(buffer.len());
                buffer[..to_copy].copy_from_slice(&data[..to_copy]);
                info!("File letto: {} -> {} bytes", path, to_copy);
                Ok(to_copy)
            }
            Err(error_code) => {
                error!(
                    "Errore durante la lettura del file {}: {}",
                    path, error_code
                );
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

        // Debug aggiuntivo per il contenuto del buffer
        let preview = if buffer.len() > 20 {
            format!("{}...{}", 
                String::from_utf8_lossy(&buffer[..10]),
                String::from_utf8_lossy(&buffer[buffer.len()-10..])
            )
        } else {
            String::from_utf8_lossy(buffer).to_string()
        };
        debug!("Buffer preview: {}", preview);

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
        },
        WriteMode::ConstrainedIO { offset } => {
            debug!("WriteMode::ConstrainedIO con offset: {}", offset);
            offset as i64
        },
        WriteMode::WriteToEOF => {
            // Per append, dobbiamo ottenere la dimensione attuale del file
            let eof_offset = match client.get_file_metadata(&path) {
                Some(meta) => {
                    debug!("WriteMode::WriteToEOF - dimensione file attuale: {}", meta.size);
                    meta.size as i64
                },
                None => {
                    debug!("WriteMode::WriteToEOF - file non trovato, offset = 0");
                    0
                }
            };
            eof_offset
        }
    };
    
    debug!("Offset finale calcolato: {}, buffer size: {}", offset, buffer.len());

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

        info!("Flush completato per handle {}", file_context);
        Ok(FileInfo::default())
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
                    // Se l'handle non è nella mappa, potrebbe essere la root directory o un handle speciale
                    debug!("File handle {} non trovato nella mappa, assumo root directory", file_handle);
                    "/".to_string() // Assumiamo che sia la root directory
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

        // Handle speciale per directory (file_handle == 0)
        if file_handle == 0 {
            debug!("set_basic_info chiamato su directory (handle 0), ignorando");
            return Ok(FileInfo::default());
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
        
        // Solo aggiorna gli attributi che sono stati effettivamente cambiati
        // 0xFFFFFFFF significa "non cambiare questo valore"
        if file_attributes.0 != 0xFFFFFFFF {
            updates.insert("mode".to_string(), serde_json::json!(file_attributes.0));
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

        // Se non ci sono aggiornamenti da fare, ritorna semplicemente le info attuali
        if updates.is_empty() {
            match client.get_file_metadata(&path) {
                Some(meta) => return Ok(meta.to_file_info()),
                None => return Err(STATUS_OBJECT_NAME_NOT_FOUND),
            }
        }

        match client.update_file_metadata(&path, serde_json::Value::Object(updates)) {
            Some(meta) => Ok(meta.to_file_info()),
            None => {
                error!("Impossibile aggiornare gli attributi per {}", path);
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

        let mut client = self.client.lock().unwrap();

        let old_path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => {
                    error!("File handle {} non trovato nella mappa", file_handle);
                    return Err(STATUS_INVALID_HANDLE);
                }
            }
        };

        let new_path = new_file_name.to_os_string().to_string_lossy().into_owned();

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

    const GET_SECURITY_DEFINED: bool = true;
    fn get_security(
        &self,
        file_context: Self::FileContext,
    ) -> Result<PSecurityDescriptor, NTSTATUS> {
        debug!("[WinFSP] get_security(file_context: {:?})", file_context);

        // Crea un security descriptor valido con permessi base
        let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
            "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
        )).map_err(|_| STATUS_ACCESS_DENIED)?;
        
        Ok(security_descriptor.as_ptr())
    }

    const SET_SECURITY_DEFINED: bool = true;
    fn set_security(
        &self,
        file_context: Self::FileContext,
        _security_information: u32,
        _modification_descriptor: PSecurityDescriptor,
    ) -> Result<(), NTSTATUS> {
        debug!("[WinFSP] set_security(file_context: {:?})", file_context);

        Ok(())
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

        if should_continue && (marker_str.is_none() || marker_str.as_ref().unwrap().as_str() <= ".")
        {
            let dir_info = DirInfo::from_str(FileInfo::default(), ".");
            should_continue = add_dir_info(dir_info);
        }

        if should_continue
            && (marker_str.is_none() || marker_str.as_ref().unwrap().as_str() <= "..")
        {
            let dir_info = DirInfo::from_str(FileInfo::default(), "..");
            should_continue = add_dir_info(dir_info);
        }

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
            path,
            entries_len + 2
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

        if file_name_str == "." || file_name_str == ".." {
            return Ok(());
        }

        let handle = file_context as u64;
        
        // Marca o rimuovi la marcatura del file per eliminazione
        let mut marked_files = self.files_marked_for_deletion.lock().unwrap();
        if delete_file {
            marked_files.insert(handle);
            info!("File marcato per eliminazione: {} (handle: {})", file_name_str, handle);
        } else {
            marked_files.remove(&handle);
            info!("Marcatura eliminazione rimossa per: {} (handle: {})", file_name_str, handle);
        }

        Ok(())
    }

    const GET_VOLUME_INFO_DEFINED: bool = false;

    const SET_VOLUME_LABEL_DEFINED: bool = false;

    const CREATE_DEFINED: bool = true;

    const OVERWRITE_DEFINED: bool = false;

    const CLOSE_DEFINED: bool = true;
    fn close(&self, _file_context: Self::FileContext) -> () {}

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
        let mut client = self.client.lock().unwrap();
        let path = {
            let handle_map = self.handle_to_path.lock().unwrap();
            match handle_map.get(&file_handle) {
                Some(p) => p.clone(),
                None => return Err(STATUS_INVALID_HANDLE),
            }
        };

        match client.get_file_metadata(&path) {
            Some(meta) => {
                if meta.file_type == RemoteFsFileType::Directory {
                    match client.list_directory(&path) {
                        Ok(entries) => {
                            let has_real_files =
                                entries.iter().any(|(name, ..)| name != "." && name != "..");

                            if has_real_files {
                                Err(STATUS_DIRECTORY_NOT_EMPTY)
                            } else {
                                Ok(())
                            }
                        }
                        Err(_) => Err(STATUS_ACCESS_DENIED),
                    }
                } else {
                    Ok(())
                }
            }
            None => Err(STATUS_OBJECT_NAME_NOT_FOUND),
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

    const DISPATCHER_STOPPED_DEFINED: bool = false;

    const RESOLVE_REPARSE_POINTS_DEFINED: bool = false;

    const GET_SECURITY_BY_NAME_DEFINED: bool = true;

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _find_reparse_point: impl Fn() -> Option<FileAttributes>,
    ) -> Result<(FileAttributes, PSecurityDescriptor, bool), NTSTATUS> {
        debug!("[WinFSP] get_security_by_name(file_name: {:?})", file_name);

        let path = file_name.to_os_string().to_string_lossy().into_owned();

        if path == "." || path == ".." {
            let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
                "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
            )).map_err(|_| STATUS_ACCESS_DENIED)?;
            
            return Ok((
                FileAttributes::DIRECTORY,
                security_descriptor.as_ptr(),
                false,
            ));
        }

        let mut client = self.client.lock().unwrap();
        match client.get_file_metadata(&path) {
            Some(metadata) => {
                let security_descriptor = SecurityDescriptor::from_wstr(u16cstr!(
                    "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
                )).map_err(|_| STATUS_ACCESS_DENIED)?;
                
                Ok((
                    FileAttributes(metadata.permissions as u32),
                    security_descriptor.as_ptr(),
                    false,
                ))
            },
            None => {
                debug!("File {} non esiste per get_security_by_name", path);
                Err(STATUS_OBJECT_NAME_NOT_FOUND)
            }
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
        
        debug!("Tentativo creazione {}: {} con flags: 0x{:x}", 
               if is_directory { "directory" } else { "file" }, 
               path, create_file_info.create_options.0);

        let mut client = self.client.lock().unwrap();
        let normalized_path = client.normalize_path_for_server(&path);

        let attrs = serde_json::json!({
            "path": normalized_path,
            "file_type": if is_directory { "Directory" } else { "RegularFile" },
            "mode": create_file_info.file_attributes.0,
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
                    // Per file regolari, aprilo immediatamente per ottenere handle
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
                    // Per directory, usa l'ino come handle
                    info!("Directory creata: {} -> ino {}", path, metadata.ino);
                    metadata.ino as u64
                };

                // Registra il mapping handle -> path
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

    fn dispatcher_stopped(&self, _normally: bool) {
        std::unreachable!("To be used, trait method must be overwritten !");
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
