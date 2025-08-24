#[cfg(target_os = "linux")]
use fuser::{FileAttr, FileType};
use serde::Deserialize;
use std::time::{Duration, SystemTime};

#[cfg(target_os = "windows")]
use winfsp_wrs::{FileAttributes, FileInfo};

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum RemoteFsFileType {
    Directory,
    RegularFile,
}
#[cfg(target_os = "linux")]
impl From<RemoteFsFileType> for fuser::FileType {
    fn from(r: RemoteFsFileType) -> Self {
        match r {
            RemoteFsFileType::Directory => FileType::Directory,
            RemoteFsFileType::RegularFile => FileType::RegularFile,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub ino: u64,
    pub size: u64,
    pub blocks: u64,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub crtime: Option<u64>,
    pub file_type: RemoteFsFileType,
    pub permissions: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub blksize: u32,
    pub flags: Option<u32>,
}

impl FileMetadata {
    #[cfg(target_os = "linux")]
    pub fn to_file_attr(&self) -> FileAttr {
        FileAttr {
            ino: self.ino,
            size: self.size,
            blocks: self.blocks,
            atime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.atime),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.mtime),
            ctime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.ctime),
            crtime: SystemTime::UNIX_EPOCH + Duration::from_secs(self.crtime.unwrap_or(self.ctime)),
            kind: fuser::FileType::from(self.file_type.clone()),
            perm: self.permissions,
            nlink: self.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: self.blksize,
            flags: self.flags.unwrap_or(0),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn to_file_info(&self) -> winfsp_wrs::FileInfo {

        let mut info = FileInfo::default();

        let attributes = match self.file_type {
            RemoteFsFileType::Directory => FileAttributes::DIRECTORY,
            RemoteFsFileType::RegularFile => FileAttributes::NORMAL,
        };

        info.set_file_attributes(attributes);

        fn to_filetime(secs: u64) -> u64 {
            // FILETIME = (UnixTime + 11644473600) * 10^7
            (secs + 11644473600) * 10_000_000
        }
        info.set_creation_time(to_filetime(self.crtime.unwrap_or(self.ctime)));
        info.set_last_access_time(to_filetime(self.atime));
        info.set_last_write_time(to_filetime(self.mtime));
        info.set_change_time(to_filetime(self.ctime));

        info.set_file_size(self.size);
        info.set_allocation_size(((self.size + (self.blksize as u64 -1)) / self.blksize as u64) * self.blksize as u64);

        info
    }
}