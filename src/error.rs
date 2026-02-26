use crate::freebsd::{jail_sys::JailError, mount::MountError, zfs::ZfsError};

#[derive(Debug, thiserror::Error)]
pub enum DailError {
    #[error("jail error: {0}")]
    Jail(#[from] JailError),

    #[error("mount error: {0}")]
    Mount(#[from] MountError),

    #[error("zfs error: {0}")]
    Zfs(#[from] ZfsError),

    #[error("jail not found: {0}")]
    JailNotFound(String),

    #[error("jail already exists: {0}")]
    JailAlreadyExists(String),

    #[error("invalid state: jail '{name}' is {status}, expected {expected}")]
    InvalidState {
        name: String,
        status: String,
        expected: String,
    },

    #[error("config error: {0}")]
    Config(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("build error: {0}")]
    Build(String),

    #[error("image error: {0}")]
    Image(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
