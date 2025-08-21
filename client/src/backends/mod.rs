#[cfg(target_os = "linux")]
pub mod fuse_backend;

#[cfg(target_os = "windows")]
pub mod winfsp_backend;

#[cfg(target_os = "linux")]
pub use fuse_backend::FuseRemoteFs;

#[cfg(target_os = "windows")]  
pub use winfsp_backend::WinFspRemoteFs;
