pub mod directory;
pub mod zfs_backend;

use crate::error::DailError;
use crate::jail::config::{GlobalConfig, JailConfig};
use crate::jail::state::JailState;
use std::path::PathBuf;

pub trait StorageBackend {
    fn prepare_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError>;

    /// Check that a base exists, returning its path. Error if not bootstrapped.
    fn check_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError>;

    /// List all bootstrapped bases as (name, path) pairs.
    fn list_bases(&self, config: &GlobalConfig) -> Result<Vec<(String, PathBuf)>, DailError>;

    fn create_jail_root(
        &self,
        config: &GlobalConfig,
        jail_config: &JailConfig,
    ) -> Result<PathBuf, DailError>;

    fn destroy(&self, config: &GlobalConfig, state: &JailState) -> Result<(), DailError>;

    fn snapshot(
        &self,
        config: &GlobalConfig,
        state: &JailState,
        tag: &str,
    ) -> Result<(), DailError>;

    fn clone_jail(
        &self,
        config: &GlobalConfig,
        source: &JailState,
        snapshot_tag: &str,
        new_name: &str,
    ) -> Result<PathBuf, DailError>;
}

/// Detect FreeBSD machine architecture for base downloads.
/// Maps `uname -m` output to FreeBSD release directory names.
pub fn freebsd_arch() -> String {
    std::process::Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "amd64".to_string())
}

pub fn get_backend(config: &GlobalConfig) -> Box<dyn StorageBackend> {
    match config.storage_backend.as_str() {
        "zfs" => Box::new(zfs_backend::ZfsBackend),
        _ => Box::new(directory::DirectoryBackend),
    }
}
