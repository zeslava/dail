use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::DailError;
use crate::jail::config::GlobalConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    #[serde(skip)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub params: HashMap<String, String>,
    #[serde(default)]
    pub limits: HashMap<String, String>,
}

impl Preset {
    pub fn load(name: &str, config: &GlobalConfig) -> Result<Option<Self>, DailError> {
        let presets_dir = config.root_dir.join("presets");
        let yaml_path = presets_dir.join(format!("{name}.yaml"));
        let toml_path = presets_dir.join(format!("{name}.toml"));

        if yaml_path.exists() {
            return Self::load_from_file(name, &yaml_path).map(Some);
        }
        if toml_path.exists() {
            return Self::load_from_file(name, &toml_path).map(Some);
        }

        Ok(builtin(name))
    }

    fn load_from_file(name: &str, path: &Path) -> Result<Self, DailError> {
        let content = std::fs::read_to_string(path)?;
        let mut preset: Self = match path.extension().and_then(|e| e.to_str()) {
            Some("yaml" | "yml") => {
                serde_yaml::from_str(&content).map_err(|e| DailError::Config(e.to_string()))?
            }
            _ => toml::from_str(&content).map_err(|e| DailError::Config(e.to_string()))?,
        };
        preset.name = name.to_string();
        Ok(preset)
    }

    pub fn list_all(config: &GlobalConfig) -> Vec<PresetInfo> {
        let mut result: HashMap<String, PresetInfo> = HashMap::new();

        for (name, preset) in builtins() {
            result.insert(
                name.clone(),
                PresetInfo {
                    name,
                    description: preset.description,
                    source: PresetSource::Builtin,
                },
            );
        }

        let presets_dir = config.root_dir.join("presets");
        if let Ok(entries) = std::fs::read_dir(&presets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_yaml = path.extension().is_some_and(|e| e == "yaml" || e == "yml");
                let is_toml = path.extension().is_some_and(|e| e == "toml");
                if !is_yaml && !is_toml {
                    continue;
                }
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let name = name.to_string();
                    // YAML wins on name conflict — skip TOML if YAML already loaded
                    if is_toml && result.get(&name).is_some_and(|p| p.source.is_user()) {
                        continue;
                    }
                    if let Ok(preset) = Self::load_from_file(&name, &path) {
                        result.insert(
                            name.clone(),
                            PresetInfo {
                                name,
                                description: preset.description,
                                source: PresetSource::User,
                            },
                        );
                    }
                }
            }
        }

        let mut list: Vec<PresetInfo> = result.into_values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

#[derive(Debug)]
pub struct PresetInfo {
    pub name: String,
    pub description: String,
    pub source: PresetSource,
}

#[derive(Debug, Clone)]
pub enum PresetSource {
    Builtin,
    User,
}

impl PresetSource {
    pub fn is_user(&self) -> bool {
        matches!(self, Self::User)
    }
}

impl std::fmt::Display for PresetSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => write!(f, "builtin"),
            Self::User => write!(f, "user"),
        }
    }
}

fn builtin(name: &str) -> Option<Preset> {
    let mut params = HashMap::new();
    match name {
        "postgres" => {
            params.insert("allow.sysvipc".to_string(), "true".to_string());
            Some(Preset {
                name: "postgres".to_string(),
                description: "PostgreSQL — enables SysV IPC".to_string(),
                params,
                limits: HashMap::new(),
            })
        }
        "nginx" => Some(Preset {
            name: "nginx".to_string(),
            description: "Nginx web server".to_string(),
            params: HashMap::new(),
            limits: HashMap::new(),
        }),
        "dev" => {
            params.insert("allow.raw_sockets".to_string(), "true".to_string());
            params.insert("allow.sysvipc".to_string(), "true".to_string());
            Some(Preset {
                name: "dev".to_string(),
                description: "Development — raw sockets + SysV IPC".to_string(),
                params,
                limits: HashMap::new(),
            })
        }
        _ => None,
    }
}

fn builtins() -> Vec<(String, Preset)> {
    ["postgres", "nginx", "dev"]
        .iter()
        .filter_map(|name| builtin(name).map(|p| (name.to_string(), p)))
        .collect()
}
