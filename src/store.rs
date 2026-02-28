use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use crate::error::DailError;
use crate::freebsd::jail_sys::JailSys;
use crate::jail::config::GlobalConfig;
use crate::jail::state::{JailState, JailStatus};

pub struct DailStore {
    jails: Vec<JailState>,
    state_path: PathBuf,
    jails_dir: PathBuf,
}

impl DailStore {
    /// Open the store for reading and writing.
    /// Uses atomic tmp-file writes (write to .tmp, rename to final name).
    /// No file locks needed—atomicity is guaranteed by filesystem rename operation.
    pub fn new(config: &GlobalConfig) -> Result<Self, DailError> {
        let state_path = config.state_path();
        let jails_dir = config.jails_dir();

        tracing::info!(
            "DailStore::new: reading state from {}",
            state_path.display()
        );
        let jails = if state_path.exists() {
            let mut content = String::new();
            File::open(&state_path)
                .map_err(|e| {
                    DailError::Other(format!(
                        "cannot read state file {}: {}",
                        state_path.display(),
                        e
                    ))
                })?
                .read_to_string(&mut content)?;

            match serde_json::from_str(&content) {
                Ok(jails) => jails,
                Err(e) => {
                    let backup = state_path.with_extension("json.corrupt");
                    tracing::warn!(
                        "state.json is corrupt ({}), backing up to {}",
                        e,
                        backup.display()
                    );
                    let _ = std::fs::copy(&state_path, &backup);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let mut store = Self {
            jails,
            state_path,
            jails_dir,
        };

        tracing::info!("DailStore: reconciling with jls");
        store.reconcile_with_jls();
        tracing::info!(
            "DailStore initialized, {} jails in store",
            store.jails.len()
        );
        Ok(store)
    }

    /// Open the store in read-only mode.
    /// No locks needed—just read the state.json file directly.
    /// Safe for concurrent reads, and avoids deadlock issues entirely.
    pub fn new_readonly(config: &GlobalConfig) -> Result<Self, DailError> {
        let state_path = config.state_path();
        let jails_dir = config.jails_dir();

        tracing::info!(
            "DailStore::new_readonly: reading state from {}",
            state_path.display()
        );
        let jails = if state_path.exists() {
            let mut content = String::new();
            File::open(&state_path)
                .map_err(|e| {
                    DailError::Other(format!(
                        "cannot read state file {}: {}",
                        state_path.display(),
                        e
                    ))
                })?
                .read_to_string(&mut content)?;

            match serde_json::from_str(&content) {
                Ok(jails) => jails,
                Err(e) => {
                    tracing::warn!("state.json is corrupt ({}), returning empty list", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        tracing::info!(
            "DailStore (readonly) initialized, {} jails in store",
            jails.len()
        );
        Ok(Self {
            jails,
            state_path,
            jails_dir,
        })
    }

    /// Cross-reference state.json with `jls` output.
    /// Fixes stale "running" status after host reboot or jail crash.
    fn reconcile_with_jls(&mut self) {
        tracing::info!("reconcile_with_jls: calling JailSys::list()");
        let running_names: HashSet<String> = JailSys::list()
            .unwrap_or_default()
            .into_iter()
            .map(|j| j.name)
            .collect();
        tracing::info!(
            "reconcile_with_jls: JailSys::list() completed, {} jails running",
            running_names.len()
        );

        let mut changed = false;
        for jail in &mut self.jails {
            if jail.status == JailStatus::Running && !running_names.contains(&jail.config.name) {
                tracing::info!(
                    "jail '{}' marked as running but not found in jls, marking as stopped",
                    jail.config.name
                );
                jail.status = JailStatus::Stopped;
                jail.jid = None;
                jail.epair = None;
                changed = true;
            }
        }

        if changed {
            let _ = self.save_index();
        }
    }

    pub fn add(&mut self, state: JailState) -> Result<(), DailError> {
        let jail_dir = self.jails_dir.join(state.name());
        std::fs::create_dir_all(&jail_dir)?;
        let jail_json = jail_dir.join("dail.json");
        let content =
            serde_json::to_string_pretty(&state).map_err(|e| DailError::Other(e.to_string()))?;
        std::fs::write(&jail_json, content)?;

        self.jails.push(state);
        self.save_index()
    }

    pub fn get(&self, name: &str) -> Option<&JailState> {
        self.jails.iter().find(|j| j.config.name == name)
    }

    pub fn update(&mut self, name: &str, f: impl FnOnce(&mut JailState)) -> Result<(), DailError> {
        let state = self
            .jails
            .iter_mut()
            .find(|j| j.config.name == name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        f(state);

        let jail_dir = self.jails_dir.join(name);
        let jail_json = jail_dir.join("dail.json");
        let content =
            serde_json::to_string_pretty(state).map_err(|e| DailError::Other(e.to_string()))?;
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

    /// Atomic write: write to temp file, then rename into place.
    fn save_index(&self) -> Result<(), DailError> {
        let content = serde_json::to_string_pretty(&self.jails)
            .map_err(|e| DailError::Other(e.to_string()))?;

        let tmp_path = self.state_path.with_extension("json.tmp");
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(content.as_bytes())?;
        tmp_file.sync_all()?;

        std::fs::rename(&tmp_path, &self.state_path)?;
        Ok(())
    }
}
