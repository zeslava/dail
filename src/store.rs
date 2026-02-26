use std::path::PathBuf;

use crate::error::DailError;
use crate::jail::config::GlobalConfig;
use crate::jail::state::JailState;

pub struct DailStore {
    jails: Vec<JailState>,
    state_path: PathBuf,
    jails_dir: PathBuf,
}

impl DailStore {
    pub fn new(config: &GlobalConfig) -> Result<Self, DailError> {
        let state_path = config.state_path();
        let jails_dir = config.jails_dir();

        let jails = if state_path.exists() {
            let content = std::fs::read_to_string(&state_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            jails,
            state_path,
            jails_dir,
        })
    }

    pub fn add(&mut self, state: JailState) -> Result<(), DailError> {
        let jail_dir = self.jails_dir.join(state.name());
        std::fs::create_dir_all(&jail_dir)?;
        let jail_json = jail_dir.join("dail.json");
        let content = serde_json::to_string_pretty(&state)
            .map_err(|e| DailError::Other(e.to_string()))?;
        std::fs::write(&jail_json, content)?;

        self.jails.push(state);
        self.save_index()
    }

    pub fn get(&self, name: &str) -> Option<&JailState> {
        self.jails.iter().find(|j| j.config.name == name)
    }

    pub fn update(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut JailState),
    ) -> Result<(), DailError> {
        let state = self
            .jails
            .iter_mut()
            .find(|j| j.config.name == name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        f(state);

        let jail_dir = self.jails_dir.join(name);
        let jail_json = jail_dir.join("dail.json");
        let content = serde_json::to_string_pretty(state)
            .map_err(|e| DailError::Other(e.to_string()))?;
        std::fs::write(&jail_json, content)?;

        self.save_index()
    }

    pub fn remove(&mut self, name: &str) -> Result<(), DailError> {
        self.jails.retain(|j| j.config.name != name);
        self.save_index()
    }

    pub fn list(&self) -> Vec<&JailState> {
        self.jails.iter().collect()
    }

    fn save_index(&self) -> Result<(), DailError> {
        let content = serde_json::to_string_pretty(&self.jails)
            .map_err(|e| DailError::Other(e.to_string()))?;
        std::fs::write(&self.state_path, content)?;
        Ok(())
    }
}
