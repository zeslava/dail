use std::path::PathBuf;

use crate::error::DailError;
use crate::jail::config::{GlobalConfig, JailConfig, JailType};
use crate::jail::state::JailState;
use crate::storage::{freebsd_arch, StorageBackend};

pub struct DirectoryBackend;

impl DirectoryBackend {
    fn base_dir(config: &GlobalConfig, release: &str) -> PathBuf {
        let arch = freebsd_arch();
        config.bases_dir().join(format!("{release}-{arch}"))
    }
}

impl StorageBackend for DirectoryBackend {
    fn prepare_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError> {
        let base_dir = Self::base_dir(config, release);

        if base_dir.exists() {
            return Ok(base_dir);
        }

        std::fs::create_dir_all(&base_dir)?;

        let arch = freebsd_arch();
        let url = format!("{}/{arch}/{release}/base.txz", config.mirror);
        eprintln!("Downloading {url}");

        let fetch = std::process::Command::new("fetch")
            .args(["-o", "-", &url])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| DailError::Storage(format!("failed to start fetch: {e}")))?;

        eprintln!("Extracting base system...");
        let tar_status = std::process::Command::new("tar")
            .args(["-xf", "-", "-C"])
            .arg(&base_dir)
            // SAFETY: stdout is Some because we set .stdout(Stdio::piped())
            .stdin(fetch.stdout.unwrap())
            .status()
            .map_err(|e| DailError::Storage(format!("failed to extract base: {e}")))?;

        if !tar_status.success() {
            std::fs::remove_dir_all(&base_dir)?;
            return Err(DailError::Storage(
                "failed to download and extract base system".to_string(),
            ));
        }

        Ok(base_dir)
    }

    fn check_base(&self, config: &GlobalConfig, release: &str) -> Result<PathBuf, DailError> {
        let base_dir = Self::base_dir(config, release);
        if !base_dir.exists() {
            tracing::error!("Base system '{}' not found at {}", release, base_dir.display());
            return Err(DailError::Storage(format!(
                "base '{release}' not found. Run: dail bootstrap {release}"
            )));
        }
        tracing::info!("Using base system '{}' from {}", release, base_dir.display());
        Ok(base_dir)
    }

    fn list_bases(&self, config: &GlobalConfig) -> Result<Vec<(String, PathBuf)>, DailError> {
        let bases_dir = config.bases_dir();
        if !bases_dir.exists() {
            return Ok(Vec::new());
        }
        let mut bases = Vec::new();
        for entry in std::fs::read_dir(&bases_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                bases.push((name.to_string(), path));
            }
        }
        bases.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(bases)
    }

    fn create_jail_root(
        &self,
        config: &GlobalConfig,
        jail_config: &JailConfig,
    ) -> Result<PathBuf, DailError> {
        let jail_dir = config.jails_dir().join(&jail_config.name);
        let root_path = jail_dir.join("root");
        std::fs::create_dir_all(&root_path)?;

        if let Some(ref release) = jail_config.base {
            let base_dir = self.check_base(config, release)?;

            match jail_config.jail_type {
                JailType::Thick => {
                    tracing::info!("Copying base system to {}", root_path.display());
                    std::fs::remove_dir_all(&root_path)?;
                    std::fs::create_dir(&root_path)?;
                    let status = std::process::Command::new("cp")
                        .arg("-a")
                        .arg(&base_dir)
                        .arg(&root_path)
                        .status()
                        .map_err(|e| DailError::Storage(format!("cp failed: {e}")))?;

                    if !status.success() {
                        return Err(DailError::Storage("cp -a failed".to_string()));
                    }
                    tracing::info!("Base system copied successfully");
                }
                JailType::Thin => {
                    tracing::info!("Preparing thin jail with skeleton directories");
                    let skeleton_dir = jail_dir.join("skeleton");
                    std::fs::create_dir_all(&skeleton_dir)?;

                    for dir in &["etc", "var", "tmp", "root"] {
                        let src = base_dir.join(dir);
                        let dst = skeleton_dir.join(dir);
                        if src.exists() {
                            let status = std::process::Command::new("cp")
                                .args(["-a"])
                                .arg(&src)
                                .arg(&skeleton_dir)
                                .status()
                                .map_err(|e| DailError::Storage(format!("cp failed: {e}")))?;
                            if !status.success() {
                                return Err(DailError::Storage(format!(
                                    "failed to copy {dir} to skeleton"
                                )));
                            }
                        } else {
                            std::fs::create_dir_all(&dst)?;
                        }
                    }

                    let fstab_path = jail_dir.join("fstab");
                    let fstab_content = format!(
                        "{base} {root} nullfs ro 0 0\n{skel} {root} nullfs rw 0 0\n",
                        base = base_dir.display(),
                        root = root_path.display(),
                        skel = skeleton_dir.display(),
                    );
                    std::fs::write(&fstab_path, fstab_content)?;
                }
            }
        }

        Ok(root_path)
    }

    fn destroy(&self, config: &GlobalConfig, state: &JailState) -> Result<(), DailError> {
        let jail_dir = config.jails_dir().join(state.name());
        if jail_dir.exists() {
            // Clear immutable/schg flags before removal (FreeBSD base sets them)
            let _ = std::process::Command::new("chflags")
                .args(["-R", "noschg"])
                .arg(&jail_dir)
                .status();
            std::fs::remove_dir_all(&jail_dir)?;
        }
        Ok(())
    }

    fn snapshot(
        &self,
        _config: &GlobalConfig,
        _state: &JailState,
        _tag: &str,
    ) -> Result<(), DailError> {
        Err(DailError::Storage(
            "snapshots are only supported with ZFS storage backend".to_string(),
        ))
    }

    fn clone_jail(
        &self,
        _config: &GlobalConfig,
        _source: &JailState,
        _snapshot_tag: &str,
        _new_name: &str,
    ) -> Result<PathBuf, DailError> {
        Err(DailError::Storage(
            "clone is only supported with ZFS storage backend".to_string(),
        ))
    }
}
