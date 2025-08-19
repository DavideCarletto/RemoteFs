pub mod filesystem;
pub mod cache;
pub mod types;

#[cfg(target_os = "windows")]
pub mod winfsp_fs;

#[cfg(target_os = "linux")]
pub use filesystem::RemoteFsClient;
pub use cache::{CacheConfig, CacheInvalidationStrategy, FileSystemCache};
pub use types::FileMetadata;