use std::path::PathBuf;

use crate::error::DailError;
use crate::freebsd::zfs::Zfs;
use crate::jail::config::{GlobalConfig, JailConfig, JailType};
use crate::jail::state::JailState;
use crate::storage::StorageBackend;

pub struct ZfsBackend;

impl ZfsBackend {
    fn pool(config: &GlobalConfig) -> Result<&str, DailError> {
        config
            .zfs_pool
            .as_deref()
            .ok_or_else(|| DailError::Config("zfs_pool not configured".to_string()))
    }

    fn base_dataset(config: &GlobalConfig, release: &str) -> Result<String, DailError> {
        let pool = Self::pool(config)?;
        Ok(format!("{pool}/dail/bases/{release}"))
    }

    fn jail_dataset(config: &GlobalConfig, name: &str) -> Result<String, DailError> {
        let pool = Self::pool(config)?;
        Ok(format!("{pool}/dail/jails/{name}"))
    }
}

impl StorageBackend for ZfsBackend {
    fn check_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError> {
        let dataset = Self::base_dataset(config, release)?;
        if !Zfs::dataset_exists(&dataset)? {
            return Err(DailError::Storage(format!(
                "base '{release}' not found. Run: dail bootstrap {release}"
            )));
        }
        Zfs::get_mountpoint(&dataset)
            .map(PathBuf::from)
            .map_err(Into::into)
    }

    fn list_bases(&self, config: &GlobalConfig) -> Result<Vec<(String, PathBuf)>, DailError> {
        let pool = Self::pool(config)?;
        let parent = format!("{pool}/dail/bases");

        let output = std::process::Command::new("zfs")
            .args([
                "list",
                "-H",
                "-o",
                "name,mountpoint",
                "-r",
                "-d",
                "1",
                &parent,
            ])
            .output()
            .map_err(|e| DailError::Storage(format!("zfs list failed: {e}")))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut bases = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 && parts[0] != parent {
                let name = parts[0].rsplit('/').next().unwrap_or(parts[0]);
                bases.push((name.to_string(), PathBuf::from(parts[1])));
            }
        }
        Ok(bases)
    }

    fn prepare_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError> {
        let dataset = Self::base_dataset(config, release)?;

        if Zfs::dataset_exists(&dataset)? {
            return Zfs::get_mountpoint(&dataset)
                .map(PathBuf::from)
                .map_err(Into::into);
        }

        Zfs::create_dataset(&dataset)?;
        let mountpoint = PathBuf::from(Zfs::get_mountpoint(&dataset)?);

        let arch = crate::storage::freebsd_arch();
        let url = format!("{}/{arch}/{release}/base.txz", config.mirror);
        tracing::info!("Downloading {}", url);

        let fetch = std::process::Command::new("fetch")
            .args(["-o", "-", &url])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| DailError::Storage(format!("fetch failed: {e}")))?;

        tracing::info!("Extracting base system to {}", mountpoint.display());
        let tar_status = std::process::Command::new("tar")
            .args(["-xf", "-", "-C"])
            .arg(&mountpoint)
            // SAFETY: stdout is Some because we set .stdout(Stdio::piped())
            .stdin(fetch.stdout.unwrap())
            .status()
            .map_err(|e| DailError::Storage(format!("tar failed: {e}")))?;

        if !tar_status.success() {
            Zfs::destroy_dataset(&dataset, true)?;
            return Err(DailError::Storage("base extraction failed".to_string()));
        }

        Zfs::snapshot(&dataset, "base")?;

        Ok(mountpoint)
    }

    fn create_jail_root(
        &self,
        config: &GlobalConfig,
        jail_config: &JailConfig,
    ) -> Result<PathBuf, DailError> {
        let release = jail_config.base.as_deref().ok_or_else(|| {
            DailError::Config("base release is required for ZFS backend".to_string())
        })?;

        let base_dataset = Self::base_dataset(config, release)?;
        self.check_base(config, release)?;

        let jail_dataset = Self::jail_dataset(config, &jail_config.name)?;

        match jail_config.jail_type {
            JailType::Thick => {
                let snapshot = format!("{base_dataset}@base");
                Zfs::clone(&snapshot, &jail_dataset)?;
            }
            JailType::Thin => {
                Zfs::create_dataset(&jail_dataset)?;
                let root_dataset = format!("{jail_dataset}/root");
                let snapshot = format!("{base_dataset}@base");
                Zfs::clone(&snapshot, &root_dataset)?;
            }
        }

        let mountpoint = Zfs::get_mountpoint(&jail_dataset)?;
        Ok(PathBuf::from(mountpoint).join("root"))
    }

    fn destroy(&self, config: &GlobalConfig, state: &JailState) -> Result<(), DailError> {
        let dataset = Self::jail_dataset(config, state.name())?;
        if Zfs::dataset_exists(&dataset)? {
            Zfs::destroy_dataset(&dataset, true)?;
        }
        Ok(())
    }

    fn snapshot(
        &self,
        config: &GlobalConfig,
        state: &JailState,
        tag: &str,
    ) -> Result<(), DailError> {
        let dataset = Self::jail_dataset(config, state.name())?;
        Zfs::snapshot(&dataset, tag)?;
        Ok(())
    }

    fn clone_jail(
        &self,
        config: &GlobalConfig,
        source: &JailState,
        snapshot_tag: &str,
        new_name: &str,
    ) -> Result<PathBuf, DailError> {
        let source_dataset = Self::jail_dataset(config, source.name())?;
        let target_dataset = Self::jail_dataset(config, new_name)?;
        let snapshot = format!("{source_dataset}@{snapshot_tag}");
        Zfs::clone(&snapshot, &target_dataset)?;
        let mountpoint = Zfs::get_mountpoint(&target_dataset)?;
        Ok(PathBuf::from(mountpoint))
    }
}
