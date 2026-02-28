use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::DailError;
use crate::jail::config::GlobalConfig;
use crate::jail::state::JailState;

/// Create a tar archive compressed with zstd.
/// Uses a safe pipe (tar stdout → zstd stdin) without shell interpolation.
pub fn tar_create_zstd(
    sources: &[(&Path, &[&str])],
    output: &Path,
) -> Result<(), DailError> {
    let mut tar_cmd = Command::new("tar");
    tar_cmd.arg("-cf").arg("-").arg("--no-fflags");
    for (dir, extra_args) in sources {
        tar_cmd.arg("-C").arg(dir);
        for a in *extra_args {
            tar_cmd.arg(a);
        }
    }
    tar_cmd.stdout(Stdio::piped());

    let tar_child = tar_cmd
        .spawn()
        .map_err(|e| DailError::Image(format!("failed to spawn tar: {e}")))?;

    let status = Command::new("zstd")
        .arg("-f")
        .arg("-o")
        .arg(output)
        .stdin(tar_child.stdout.expect("tar stdout is piped"))
        .status()
        .map_err(|e| DailError::Image(format!("failed to spawn zstd: {e}")))?;

    if !status.success() {
        return Err(DailError::Image("tar/zstd failed".to_string()));
    }
    Ok(())
}

/// Extract a zstd-compressed tar archive.
/// Uses a safe pipe (zstd stdout → tar stdin) without shell interpolation.
fn zstd_extract_tar(
    archive: &Path,
    dest: &Path,
    extra_tar_args: &[&str],
) -> Result<(), DailError> {
    let mut zstd_cmd = Command::new("zstd");
    zstd_cmd
        .arg("-d")
        .arg(archive)
        .arg("--stdout")
        .stdout(Stdio::piped());

    let zstd_child = zstd_cmd
        .spawn()
        .map_err(|e| DailError::Image(format!("failed to spawn zstd: {e}")))?;

    let mut tar_cmd = Command::new("tar");
    tar_cmd.arg("-xf").arg("-").arg("-C").arg(dest);
    for a in extra_tar_args {
        tar_cmd.arg(a);
    }
    tar_cmd.stdin(zstd_child.stdout.expect("zstd stdout is piped"));

    let status = tar_cmd
        .status()
        .map_err(|e| DailError::Image(format!("failed to spawn tar: {e}")))?;

    if !status.success() {
        return Err(DailError::Image("zstd/tar extraction failed".to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageManifest {
    pub name: String,
    pub tag: String,
    pub base: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub limits: HashMap<String, String>,
    #[serde(default)]
    pub persist: bool,
    pub cmd: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct ImageStore {
    images_dir: PathBuf,
}

impl ImageStore {
    pub fn new(global: &GlobalConfig) -> Self {
        Self {
            images_dir: global.images_dir(),
        }
    }

    pub fn save(
        &self,
        state: &JailState,
        tag: &str,
        output: Option<&Path>,
    ) -> Result<PathBuf, DailError> {
        let manifest = ImageManifest {
            name: state.config.name.clone(),
            tag: tag.to_string(),
            base: state.config.base.clone(),
            params: state.config.params.clone(),
            limits: state.config.limits.clone(),
            persist: state.config.persist,
            cmd: state.config.cmd.clone(),
            created_at: Utc::now(),
        };

        let tmp_dir = tempfile::tempdir()?;
        let manifest_path = tmp_dir.path().join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| DailError::Image(e.to_string()))?;
        std::fs::write(&manifest_path, &manifest_json)?;

        let archive_name = format!("{}-{}.tar.zst", manifest.name, tag);
        let output_path = output
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&archive_name));

        tar_create_zstd(
            &[
                (state.root_path.as_path(), &["."]),
                (tmp_dir.path(), &["manifest.json"]),
            ],
            &output_path,
        )?;

        // Register image in images_dir so `image ls` and `image inspect` find it
        let image_dir = self.images_dir.join(&manifest.name).join(tag);
        std::fs::create_dir_all(&image_dir)?;
        std::fs::write(image_dir.join("manifest.json"), &manifest_json)?;
        std::fs::copy(&output_path, image_dir.join("image.tar.zst"))?;

        Ok(output_path)
    }

    pub fn load(&self, archive: &Path) -> Result<ImageManifest, DailError> {
        let tmp_dir = tempfile::tempdir()?;

        // Extract manifest first to get name/tag
        zstd_extract_tar(archive, tmp_dir.path(), &["manifest.json"])?;

        let manifest_path = tmp_dir.path().join("manifest.json");
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: ImageManifest =
            serde_json::from_str(&content).map_err(|e| DailError::Image(e.to_string()))?;

        let image_dir = self.images_dir.join(&manifest.name).join(&manifest.tag);
        std::fs::create_dir_all(&image_dir)?;

        // Store manifest
        std::fs::write(image_dir.join("manifest.json"), &content)?;

        // Copy archive
        std::fs::copy(archive, image_dir.join("image.tar.zst"))?;

        Ok(manifest)
    }

    pub fn list(&self) -> Result<Vec<ImageManifest>, DailError> {
        let mut images = Vec::new();

        if !self.images_dir.exists() {
            return Ok(images);
        }

        for name_entry in std::fs::read_dir(&self.images_dir)? {
            let name_entry = name_entry?;
            if !name_entry.file_type()?.is_dir() {
                continue;
            }
            for tag_entry in std::fs::read_dir(name_entry.path())? {
                let tag_entry = tag_entry?;
                let manifest_path = tag_entry.path().join("manifest.json");
                if manifest_path.exists() {
                    let content = std::fs::read_to_string(&manifest_path)?;
                    if let Ok(manifest) = serde_json::from_str::<ImageManifest>(&content) {
                        images.push(manifest);
                    }
                }
            }
        }

        images.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(images)
    }

    pub fn load_manifest(&self, name: &str, tag: &str) -> Result<ImageManifest, DailError> {
        let manifest_path = self.images_dir.join(name).join(tag).join("manifest.json");
        if !manifest_path.exists() {
            return Err(DailError::Image(format!(
                "image not found: {}:{}",
                name, tag
            )));
        }
        let content = std::fs::read_to_string(&manifest_path)?;
        serde_json::from_str(&content).map_err(|e| DailError::Image(e.to_string()))
    }

    pub fn extract(&self, name: &str, tag: &str, dest: &Path) -> Result<ImageManifest, DailError> {
        let manifest = self.load_manifest(name, tag)?;
        let archive_path = self.images_dir.join(name).join(tag).join("image.tar.zst");
        if !archive_path.exists() {
            return Err(DailError::Image(format!(
                "image archive not found: {}:{}",
                name, tag
            )));
        }

        std::fs::create_dir_all(dest)?;

        zstd_extract_tar(&archive_path, dest, &["--exclude", "manifest.json", "--no-fflags"])?;

        Ok(manifest)
    }

    pub fn ensure_rootfs(&self, name: &str, tag: &str) -> Result<PathBuf, DailError> {
        let rootfs_dir = self.images_dir.join(name).join(tag).join("rootfs");
        if rootfs_dir.exists() && rootfs_dir.read_dir()?.next().is_some() {
            return Ok(rootfs_dir);
        }

        let archive_path = self.images_dir.join(name).join(tag).join("image.tar.zst");
        if !archive_path.exists() {
            return Err(DailError::Image(format!(
                "image archive not found: {}:{}",
                name, tag
            )));
        }

        std::fs::create_dir_all(&rootfs_dir)?;

        if let Err(e) = zstd_extract_tar(&archive_path, &rootfs_dir, &["--exclude", "manifest.json", "--no-fflags"]) {
            let _ = std::fs::remove_dir_all(&rootfs_dir);
            return Err(e);
        }

        Ok(rootfs_dir)
    }

    pub fn remove(&self, name: &str, tag: &str) -> Result<(), DailError> {
        let image_dir = self.images_dir.join(name).join(tag);
        if !image_dir.exists() {
            return Err(DailError::Image(format!(
                "image not found: {}:{}",
                name, tag
            )));
        }
        std::fs::remove_dir_all(&image_dir)?;

        // Clean up empty parent dir
        let name_dir = self.images_dir.join(name);
        if name_dir.exists() && name_dir.read_dir()?.next().is_none() {
            std::fs::remove_dir(&name_dir)?;
        }

        Ok(())
    }
}
