use fuser::{FileAttr, FileType, Filesystem};
use log::{debug, error, info, warn};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::{Duration, SystemTime};

const MAX_NAME_LENGTH: u32 = 255;

#[derive(Deserialize)]
struct FileMetadata {
    ino: u64,
    size: u64,
    blocks: u64,
    atime: u64,
    mtime: u64,
    ctime: u64,
    crtime: Option<u64>,
    file_type: FileType,
    permissions: u16,
    nlink: u32,
    uid: u32,
    gid: u32,
    blksize: u32,
    flags: Option<u32>,
}

impl FileMetadata {
    fn to_file_attr(&self) -> FileAttr {
        FileAttr {
            ino: self.ino,
            size: self.size,
            blocks: self.blocks,
            atime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.atime),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.mtime),
            ctime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.ctime),
            crtime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.crtime.unwrap_or(self.ctime)),
            kind: self.file_type,
            perm: self.permissions,
            nlink: self.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: self.blksize,
            flags: self.flags.unwrap_or(0),
        }
    }
}

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

    fn update_file_attributes(&self, path: &str, updates: serde_json::Value) -> Option<FileMetadata> {
        debug!("Aggiornamento attributi per: {} con {:?}", path, updates);
        
        let client = Client::new();
        let url = format!("{}/metadata?path={}", self.api_url, path);
        
        match client.patch(&url).json(&updates).send() {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<FileMetadata>() {
                    Ok(metadata) => {
                        info!("Attributi aggiornati per {}: inode {}", path, metadata.ino);
                        Some(metadata)
                    }
                    Err(e) => {
                        error!("Errore parsing JSON per {}: {}", path, e);
                        None
                    }
                }
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
}

impl Filesystem for RemoteFsClient {
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        let health_url = format!("{}/health", self.api_url);
        let client = Client::new();
        match client.get(&health_url).send() {
            Ok(resp) if resp.status().is_success() => {
                config.set_max_readahead(128 * 1024).ok();
                config.set_max_write(128 * 1024).ok();
                info!("Remote FS client initialized successfully.");
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

        // Converti OsStr in String
        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                error!("Nome file non valido: {:?}", name);
                reply.error(libc::EINVAL);
                return;
            }
        };

        // Costruisci il percorso completo
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

        // Richiedi metadati al server
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

    fn forget(&mut self, _req: &fuser::Request<'_>, _ino: u64, _nlookup: u64) {} //implement only if filesystem implements inode lifetimes

    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: Option<u64>,
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

        match self.get_file_metadata(&path) {
            Some(metadata) => {
                let file_attr = metadata.to_file_attr();
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
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let path = match self.inode_to_path(ino){
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return
            }
        };

        let updates = serde_json::json!({
            "mode": mode,
            "uid": uid,
            "gid": gid,
            "size": size,
            "flags": flags,
        });
        
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

    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!("[Not Implemented] readlink(ino: {:#x?})", ino);
        reply.error(libc::ENOSYS);
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
            "[Not Implemented] mknod(parent: {:#x?}, name: {:?}, mode: {}, \
            umask: {:#x?}, rdev: {})",
            parent, name, mode, umask, rdev
        );
        reply.error(libc::ENOSYS);
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
            "[Not Implemented] mkdir(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?})",
            parent, name, mode, umask
        );
        reply.error(libc::ENOSYS);
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] unlink(parent: {:#x?}, name: {:?})",
            parent, name,
        );
        reply.error(libc::ENOSYS);
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] rmdir(parent: {:#x?}, name: {:?})",
            parent, name,
        );
        reply.error(libc::ENOSYS);
    }

    fn symlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        link_name: &std::ffi::OsStr,
        target: &std::path::Path,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] symlink(parent: {:#x?}, link_name: {:?}, target: {:?})",
            parent, link_name, target,
        );
        reply.error(libc::EPERM);
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
            "[Not Implemented] rename(parent: {:#x?}, name: {:?}, newparent: {:#x?}, \
            newname: {:?}, flags: {})",
            parent, name, newparent, newname, flags,
        );
        reply.error(libc::ENOSYS);
    }

    fn link(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        newparent: u64,
        newname: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] link(ino: {:#x?}, newparent: {:#x?}, newname: {:?})",
            ino, newparent, newname
        );
        reply.error(libc::EPERM);
    }

    fn open(&mut self, _req: &fuser::Request<'_>, _ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        reply.opened(0, 0);
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
        warn!(
            "[Not Implemented] read(ino: {:#x?}, fh: {}, offset: {}, size: {}, \
            flags: {:#x?}, lock_owner: {:?})",
            ino, fh, offset, size, flags, lock_owner
        );
        reply.error(libc::ENOSYS);
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
            "[Not Implemented] write(ino: {:#x?}, fh: {}, offset: {}, data.len(): {}, \
            write_flags: {:#x?}, flags: {:#x?}, lock_owner: {:?})",
            ino,
            fh,
            offset,
            data.len(),
            write_flags,
            flags,
            lock_owner
        );
        reply.error(libc::ENOSYS);
    }

    fn flush(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] flush(ino: {:#x?}, fh: {}, lock_owner: {:?})",
            ino, fh, lock_owner
        );
        reply.error(libc::ENOSYS);
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

    fn fsync(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fsync(ino: {:#x?}, fh: {}, datasync: {})",
            ino, fh, datasync
        );
        reply.error(libc::ENOSYS);
    }

    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        reply.opened(0, 0);
    }

    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectory,
    ) {
        warn!(
            "[Not Implemented] readdir(ino: {:#x?}, fh: {}, offset: {})",
            ino, fh, offset
        );
        reply.error(libc::ENOSYS);
    }

    fn readdirplus(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectoryPlus,
    ) {
        debug!(
            "[Not Implemented] readdirplus(ino: {:#x?}, fh: {}, offset: {})",
            ino, fh, offset
        );
        reply.error(libc::ENOSYS);
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

    fn fsyncdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fsyncdir(ino: {:#x?}, fh: {}, datasync: {})",
            ino, fh, datasync
        );
        reply.error(libc::ENOSYS);
    }

    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }

    fn setxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        _value: &[u8],
        flags: i32,
        position: u32,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] setxattr(ino: {:#x?}, name: {:?}, flags: {:#x?}, position: {})",
            ino, name, flags, position
        );
        reply.error(libc::ENOSYS);
    }

    fn getxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!(
            "[Not Implemented] getxattr(ino: {:#x?}, name: {:?}, size: {})",
            ino, name, size
        );
        reply.error(libc::ENOSYS);
    }

    fn listxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!(
            "[Not Implemented] listxattr(ino: {:#x?}, size: {})",
            ino, size
        );
        reply.error(libc::ENOSYS);
    }

    fn removexattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] removexattr(ino: {:#x?}, name: {:?})",
            ino, name
        );
        reply.error(libc::ENOSYS);
    }

    fn access(&mut self, _req: &fuser::Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        debug!("[Not Implemented] access(ino: {:#x?}, mask: {})", ino, mask);
        reply.error(libc::ENOSYS);
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
            "[Not Implemented] create(parent: {:#x?}, name: {:?}, mode: {}, umask: {:#x?}, \
            flags: {:#x?})",
            parent, name, mode, umask, flags
        );
        reply.error(libc::ENOSYS);
    }

    fn getlk(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
        reply: fuser::ReplyLock,
    ) {
        debug!(
            "[Not Implemented] getlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {})",
            ino, fh, lock_owner, start, end, typ, pid
        );
        reply.error(libc::ENOSYS);
    }

    fn setlk(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
        sleep: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] setlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {}, sleep: {})",
            ino, fh, lock_owner, start, end, typ, pid, sleep
        );
        reply.error(libc::ENOSYS);
    }

    fn bmap(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        blocksize: u32,
        idx: u64,
        reply: fuser::ReplyBmap,
    ) {
        debug!(
            "[Not Implemented] bmap(ino: {:#x?}, blocksize: {}, idx: {})",
            ino, blocksize, idx,
        );
        reply.error(libc::ENOSYS);
    }

    fn ioctl(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        flags: u32,
        cmd: u32,
        in_data: &[u8],
        out_size: u32,
        reply: fuser::ReplyIoctl,
    ) {
        debug!(
            "[Not Implemented] ioctl(ino: {:#x?}, fh: {}, flags: {}, cmd: {}, \
            in_data.len(): {}, out_size: {})",
            ino,
            fh,
            flags,
            cmd,
            in_data.len(),
            out_size,
        );
        reply.error(libc::ENOSYS);
    }

    fn fallocate(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        length: i64,
        mode: i32,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fallocate(ino: {:#x?}, fh: {}, offset: {}, \
            length: {}, mode: {})",
            ino, fh, offset, length, mode
        );
        reply.error(libc::ENOSYS);
    }

    fn lseek(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: fuser::ReplyLseek,
    ) {
        debug!(
            "[Not Implemented] lseek(ino: {:#x?}, fh: {}, offset: {}, whence: {})",
            ino, fh, offset, whence
        );
        reply.error(libc::ENOSYS);
    }

    fn copy_file_range(
        &mut self,
        _req: &fuser::Request<'_>,
        ino_in: u64,
        fh_in: u64,
        offset_in: i64,
        ino_out: u64,
        fh_out: u64,
        offset_out: i64,
        len: u64,
        flags: u32,
        reply: fuser::ReplyWrite,
    ) {
        debug!(
            "[Not Implemented] copy_file_range(ino_in: {:#x?}, fh_in: {}, \
            offset_in: {}, ino_out: {:#x?}, fh_out: {}, offset_out: {}, \
            len: {}, flags: {})",
            ino_in, fh_in, offset_in, ino_out, fh_out, offset_out, len, flags
        );
        reply.error(libc::ENOSYS);
    }
}
