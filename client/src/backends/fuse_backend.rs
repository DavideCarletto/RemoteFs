#[cfg(target_os = "linux")]
use fuser::{Filesystem};
use log::{debug, error, info, warn};
use std::time::{Duration, SystemTime};
use serde_json::json;
use crate::client::RemoteFsClient;

const MAX_NAME_LENGTH: u32 = 255;

pub struct FuseRemoteFs {
    client: RemoteFsClient,
}

#[cfg(target_os = "linux")]
impl FuseRemoteFs {
    /// Crea una nuova istanza del filesystem FUSE
    pub fn new(api_url: String) -> Self {
        debug!("Inizializzazione FuseRemoteFs per API: {}", api_url);

        let client = RemoteFsClient::new(api_url);
        
        Self { client }
    }

    pub fn client(&self) -> &RemoteFsClient {
        &self.client
    }

    pub fn client_mut(&mut self) -> &mut RemoteFsClient {
        &mut self.client
    }
}

#[cfg(target_os = "linux")]
impl Filesystem for FuseRemoteFs {
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {

        match self.client.test_connection() {
            Ok(_) => {
                config.set_max_readahead(1024 * 1024).ok();
                config.set_max_write(1024 * 1024).ok();

                info!("Client del filesystem remoto inizializzato con successo.");
                Ok(())
            }
            Err(e) => {
                error!("{}", e);
                Err(libc::EIO)
            }
        }
    }
    fn destroy(&mut self) {
        
        debug!("Cache stats: {:?}", self.client.cache().get_stats());

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

        let full_path = match self.client.build_path(parent, name_str) {
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

        match self.client.get_file_metadata(&full_path) {
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

        let path = match self.client.inode_to_path(ino) {
            Some(p) => p,
            None => {
                error!("Impossibile trovare il percorso per inode {:#x?}", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };

        match self.client.get_file_metadata(&path) {
            Some(metadata) => {
                let file_attr = metadata.to_file_attr();
                info!("Recuperati attributi per inode {:#x?}: {:?}", ino, file_attr);
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
        debug!("setattr(ino: {:#x?}, mode: {:?}, uid: {:?}, gid: {:?}, size: {:?}, atime: {:?}, mtime: {:?}, flags: {:?})",
            ino, mode, uid, gid, size, atime, mtime, flags);

        let path = match self.client.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

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

        match self.client.update_file_attributes(path.as_str(), updates.clone()) {
            Some(metadata) => {
                let file_attr = metadata.to_file_attr();
                info!("Attributi aggiornati per {}: {:?}", path, file_attr);
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

        let full_path = match self.client.build_path(parent, name_str) {
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
        match self.client.create_filesystem_object(
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
                info!("File creato: {} -> inode {}", full_path, metadata.ino);
                reply.entry(&std::time::Duration::from_secs(1), &file_attr, 0);
            }
            Err(error_code) => {
                error!("Errore durante la creazione del file {}: {}", full_path, error_code);
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

        let full_path = match self.client.build_path(parent, name_str) {
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

        match self.client.create_filesystem_object(
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
                info!("Directory creata: {} -> inode {}", full_path, metadata.ino);
                reply.entry(&std::time::Duration::from_secs(1), &file_attr, 0);
            }
            Err(error_code) => {
                error!("Errore durante la creazione della directory {}: {}", full_path, error_code);
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

        let full_path = match self.client.build_path(parent, name_str) {
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

        match self.client.remove_filesystem_object(&full_path, false) {
            Ok(()) => {
                info!("File rimosso: {}", full_path);
                reply.ok();
            }
            Err(error_code) => {
                error!("Errore durante la rimozione del file {}: {}", full_path, error_code);
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

        let full_path = match self.client.build_path(parent, name_str) {
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

        match self.client.remove_filesystem_object(&full_path, true) {
            Ok(()) => {
                info!("Directory rimossa: {}", full_path);
                reply.ok();
            }
            Err(error_code) => {
                error!("Errore durante la rimozione della directory {}: {}", full_path, error_code);
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

        let old_path = match self.client.build_path(parent, name_str) {
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

        let new_path = match self.client.build_path(newparent, newname_str) {
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

        match self.client.rename_filesystem_object(&old_path, &new_path) {
            Ok(()) => {
                info!("File rinominato: {} -> {}", old_path, new_path);
                reply.ok();
            }
            Err(error_code) => {
                error!("Errore durante la rimozione del file {}: {}", old_path, error_code);
                reply.error(error_code);
            }
        }
    }

    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug!("open(ino: {:#x}, flags: {:#x})", ino, flags);

        let path = match self.client.inode_to_path(ino) {
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

        match self.client.open_file(&path, flags) {
            Ok(file_handle) => {
                info!("File aperto: {} -> {}", path, file_handle);
                reply.opened(file_handle, 0);
            }
            Err(error_code) => {
                error!("Errore durante l'apertura del file {}: {}", path, error_code);
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

        let path = match self.client.inode_to_path(ino) {
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

        match self.client.read_file(&path, fh, offset, size) {
            Ok(data) => {
                info!("File letto: {} -> {}", path, data.len());
                reply.data(&data);
            }
            Err(error_code) => {
                error!("Errore durante la lettura del file {}: {}", path, error_code);
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

        let path = match self.client.inode_to_path(ino) {
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

        match self.client.write_file(&path, fh, offset, data) {
            Ok(bytes_written) => {
                info!("File scritto: {} -> {}", path, bytes_written);
                reply.written(bytes_written);
            }
            Err(error_code) => {
                error!("Errore durante la scrittura del file {}: {}", path, error_code);
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

        let path = match self.client.inode_to_path(ino) {
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
        match self.client.get_file_metadata(&path) {
            Some(metadata) if fuser::FileType::from(metadata.file_type.clone()) == fuser::FileType::Directory => {
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

        let entries = match self.client.list_directory(&path) {
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

        for (name, entry_ino, file_type) in entries {
            full_entries.push((name, entry_ino, fuser::FileType::from(file_type)));
        }

        for (i, (name, entry_ino, file_type)) in
            full_entries.iter().enumerate().skip(offset as usize)
        {
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

        let path = match self.client.inode_to_path(ino) {
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

        match self.client.get_file_metadata(&path) {
            Some(metadata) => {
                let file_mode = metadata.permissions as i32;
                let uid = _req.uid();
                let gid = _req.gid();

                let mut allowed = true;

                if uid == 0 {
                    reply.ok();
                    return;
                }

                if mask & libc::R_OK != 0 {
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o400) != 0;
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o040) != 0;
                    } else {
                        allowed &= (file_mode & 0o004) != 0;
                    }
                }

                if mask & libc::W_OK != 0 {
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o200) != 0;
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o020) != 0;
                    } else {
                        allowed &= (file_mode & 0o002) != 0;
                    }
                }

                if mask & libc::X_OK != 0 {
                    if uid == metadata.uid {
                        allowed &= (file_mode & 0o100) != 0;
                    } else if gid == metadata.gid {
                        allowed &= (file_mode & 0o010) != 0;
                    } else {
                        allowed &= (file_mode & 0o001) != 0;
                    }
                }

                if allowed {
                    reply.ok();
                } else {
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

        let full_path = match self.client.build_path(parent, name_str) {
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

        let file_type = "RegularFile";
        let effective_flags = flags | libc::O_WRONLY;
        let effective_mode = (mode & !umask) | libc::S_IFREG;
        let permissions = effective_mode & !libc::S_IFMT;

        if let Some(_) = self.client.get_file_metadata(&full_path) {
            error!("File già esistente: {}", full_path);
            reply.error(libc::EEXIST);
            return;
        }

        match self.client.create_filesystem_object(
            &full_path,
            file_type,
            permissions,
            _req.uid(),
            _req.gid(),
            0,
            umask,
        ) {
            Ok(metadata) => {
                match self.client.open_file(&full_path, effective_flags) {
                    Ok(file_handle) => {
                        let file_attr = metadata.to_file_attr();
                        info!(
                            "File creato e aperto: {} -> inode {}, handle {}",
                            full_path, metadata.ino, file_handle
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
                        let _ = self.client.remove_filesystem_object(&full_path, false);
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
        let dir_handle = 0;
        reply.opened(dir_handle, 0);
    }
}