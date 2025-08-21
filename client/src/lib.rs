// Core modules - Architettura modulare
pub mod client;
pub mod backends;
pub mod cache;
pub mod types;

// Re-exports principali
pub use client::RemoteFsClient;
pub use cache::{CacheConfig, CacheInvalidationStrategy, FileSystemCache};
pub use types::FileMetadata;

// Re-exports dei backends (architettura principale)
pub use backends::*;