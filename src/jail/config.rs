use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::DailError;
use crate::network::NetworkConfig;

/// Global dail configuration, stored at /usr/local/etc/dail/config.yaml (TOML fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_root_dir")]
    pub root_dir: PathBuf,

    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zfs_pool: Option<String>,

    #[serde(default)]
    pub default_network: NetworkConfig,

    #[serde(default = "default_alias_interface")]
    pub alias_interface: String,

    #[serde(default = "default_ip_pool")]
    pub ip_pool: String,

    #[serde(default = "default_mirror")]
    pub mirror: String,

    #[serde(default = "default_base")]
    pub default_base: String,
}

fn default_root_dir() -> PathBuf {
    PathBuf::from("/var/db/dail")
}

fn default_config_dir() -> PathBuf {
    PathBuf::from("/usr/local/etc/dail")
}

fn default_storage_backend() -> String {
    "directory".to_string()
}

fn default_alias_interface() -> String {
    "lo0".to_string()
}

fn default_ip_pool() -> String {
    "10.100.0.0/24".to_string()
}

fn default_mirror() -> String {
    "https://download.freebsd.org/releases".to_string()
}

fn default_base() -> String {
    "15.0-RELEASE".to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            root_dir: default_root_dir(),
            storage_backend: default_storage_backend(),
            zfs_pool: None,
            default_network: NetworkConfig::default(),
            alias_interface: default_alias_interface(),
            ip_pool: default_ip_pool(),
            mirror: default_mirror(),
            default_base: default_base(),
        }
    }
}

impl GlobalConfig {
    pub fn config_path(&self) -> PathBuf {
        default_config_dir().join("config.yaml")
    }

    pub fn bases_dir(&self) -> PathBuf {
        self.root_dir.join("bases")
    }

    pub fn jails_dir(&self) -> PathBuf {
        self.root_dir.join("jails")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root_dir.join("state.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root_dir.join("cache")
    }

    pub fn pkg_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("pkg")
    }

    pub fn pkg_repos_dir(&self) -> PathBuf {
        self.cache_dir().join("pkg_repos")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.root_dir.join("images")
    }

    pub fn load() -> Result<Self, DailError> {
        let dir = default_config_dir();
        let yaml_path = dir.join("config.yaml");
        let toml_path = dir.join("config.toml");

        if yaml_path.exists() {
            return Self::load_from(&yaml_path);
        }
        if toml_path.exists() {
            return Self::load_from(&toml_path);
        }

        // No config file found — check if dail was ever initialized
        let root = default_root_dir();
        if !root.exists() {
            return Err(DailError::Config(
                "dail is not initialized. Run `dail config init` first.".to_string(),
            ));
        }

        Ok(Self::default())
    }

    pub fn load_from(path: &Path) -> Result<Self, DailError> {
        let content = std::fs::read_to_string(path)?;
        match path.extension().and_then(|e| e.to_str()) {
            Some("yaml" | "yml") => {
                serde_yaml::from_str(&content).map_err(|e| DailError::Config(e.to_string()))
            }
            _ => toml::from_str(&content).map_err(|e| DailError::Config(e.to_string())),
        }
    }

    pub fn save(&self) -> Result<(), DailError> {
        let path = self.config_path();
        let content = serde_yaml::to_string(self).map_err(|e| DailError::Config(e.to_string()))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn init(&self) -> Result<(), DailError> {
        std::fs::create_dir_all(default_config_dir())?;
        std::fs::create_dir_all(&self.root_dir)?;
        std::fs::create_dir_all(self.bases_dir())?;
        std::fs::create_dir_all(self.jails_dir())?;
        self.save()?;

        let state_path = self.state_path();
        if !state_path.exists() {
            std::fs::write(&state_path, "[]")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JailType {
    Thick,
    #[default]
    Thin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub readonly: bool,
    #[serde(default = "default_mount_type")]
    pub fs_type: String,
}

fn default_mount_type() -> String {
    "nullfs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailConfig {
    pub name: String,
    pub hostname: Option<String>,
    #[serde(default)]
    pub jail_type: JailType,
    pub base: Option<String>,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub mounts: Vec<MountSpec>,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub limits: HashMap<String, String>,
    #[serde(default)]
    pub persist: bool,
    #[serde(default)]
    pub auto_remove: bool,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
}

impl JailConfig {
    pub fn new(name: String) -> Self {
        Self {
            name,
            hostname: None,
            jail_type: JailType::default(),
            base: None,
            network: NetworkConfig::default(),
            mounts: Vec::new(),
            params: HashMap::new(),
            limits: HashMap::new(),
            persist: false,
            auto_remove: false,
            cmd: None,
            log_file: None,
            image_ref: None,
        }
    }
}
