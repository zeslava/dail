use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::DailError;
use crate::jail::config::GlobalConfig;
use crate::jail::state::JailState;

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

        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "tar -cf - --no-fflags -C {} . -C {} manifest.json | zstd -f -o {}",
                state.root_path.display(),
                tmp_dir.path().display(),
                output_path.display(),
            ))
            .status()?;

        if !status.success() {
            return Err(DailError::Image("tar/zstd failed".to_string()));
        }

        Ok(output_path)
    }

    pub fn load(&self, archive: &Path) -> Result<ImageManifest, DailError> {
        let tmp_dir = tempfile::tempdir()?;

        // Extract manifest first to get name/tag
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "zstd -d {} --stdout | tar -xf - -C {} manifest.json",
                archive.display(),
                tmp_dir.path().display(),
            ))
            .status()?;

        if !status.success() {
            return Err(DailError::Image(
                "failed to extract manifest from archive".to_string(),
            ));
        }

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

        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "zstd -d {} --stdout | tar -xf - -C {} --exclude manifest.json --no-fflags",
                archive_path.display(),
                dest.display(),
            ))
            .status()?;

        if !status.success() {
            return Err(DailError::Image("failed to extract image".to_string()));
        }

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

        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "zstd -d {} --stdout | tar -xf - -C {} --exclude manifest.json --no-fflags",
                archive_path.display(),
                rootfs_dir.display(),
            ))
            .status()?;

        if !status.success() {
            // Clean up on failure
            let _ = std::fs::remove_dir_all(&rootfs_dir);
            return Err(DailError::Image("failed to extract image rootfs".to_string()));
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
