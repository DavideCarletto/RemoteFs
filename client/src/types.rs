
// Versione Linux: usa i tipi FUSE
#[cfg(target_os = "linux")]
use fuser::{FileAttr, FileType};
#[cfg(target_os = "linux")]
use serde::Deserialize;
#[cfg(target_os = "linux")]
use std::time::{Duration, SystemTime};

#[cfg(target_os = "linux")]
#[derive(Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub crtime: Option<u64>,
    pub file_type: FileType,
    pub permissions: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub blksize: u32,
    pub flags: Option<u32>,
}

#[cfg(target_os = "linux")]
impl FileMetadata {
    pub fn to_file_attr(&self) -> FileAttr {
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

// Versione Windows: solo i campi base, senza tipi FUSE
#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub crtime: Option<u64>,
    pub file_type: String,
    pub permissions: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub blksize: u32,
    pub flags: Option<u32>,
}
