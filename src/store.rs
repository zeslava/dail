use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use crate::error::DailError;
use crate::freebsd::jail_sys::JailSys;
use crate::jail::config::GlobalConfig;
use crate::jail::state::{JailState, JailStatus};

pub struct DailStore {
    jails: Vec<JailState>,
    state_path: PathBuf,
    jails_dir: PathBuf,
    /// Held for the lifetime of DailStore to prevent concurrent access
    _lock_file: File,
}

/// Acquire an exclusive flock on the given path. Creates the file if needed.
fn acquire_lock(path: &std::path::Path) -> Result<File, DailError> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| {
            DailError::Other(format!(
                "failed to open lock file {}: {}",
                path.display(),
                e
            ))
        })?;

    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(DailError::Other(format!(
            "failed to acquire exclusive lock on {}",
            path.display()
        )));
    }

    Ok(file)
}

/// Acquire a shared flock on the given path (read-only).
fn acquire_shared_lock(path: &std::path::Path) -> Result<File, DailError> {
    let file = OpenOptions::new().read(true).open(path).map_err(|e| {
        DailError::Other(format!(
            "cannot read lock file {}: {}. Try running with doas/sudo if needed.",
            path.display(),
            e
        ))
    })?;

    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH) };
    if rc != 0 {
        return Err(DailError::Other(format!(
            "failed to acquire shared lock on {}",
            path.display()
        )));
    }

    Ok(file)
}

impl DailStore {
    pub fn new(config: &GlobalConfig) -> Result<Self, DailError> {
        Self::new_internal(config, false)
    }

    /// Open the store in read-only mode with shared lock.
    /// Suitable for commands that only read jail state (ls, inspect, logs).
    pub fn new_readonly(config: &GlobalConfig) -> Result<Self, DailError> {
        Self::new_internal(config, true)
    }

    fn new_internal(config: &GlobalConfig, readonly: bool) -> Result<Self, DailError> {
        let state_path = config.state_path();
        let jails_dir = config.jails_dir();

        let lock_path = state_path.with_extension("lock");
        let lock_file = if readonly {
            // For readonly, we need shared lock only if lock file exists
            if lock_path.exists() {
                acquire_shared_lock(&lock_path)?
            } else {
                // No lock file yet, no jails to list
                return Ok(Self {
                    jails: Vec::new(),
                    state_path,
                    jails_dir,
                    _lock_file: OpenOptions::new().read(true).open("/dev/null").unwrap(), // /dev/null always exists
                });
            }
        } else {
            acquire_lock(&lock_path)?
        };

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
                    // Backup corrupt file instead of silently discarding
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
            _lock_file: lock_file,
        };

        if !readonly {
            store.reconcile_with_jls();
        }

        Ok(store)
    }

    /// Cross-reference state.json with `jls` output.
    /// Fixes stale "running" status after host reboot or jail crash.
    fn reconcile_with_jls(&mut self) {
        let running_names: HashSet<String> = JailSys::list()
            .unwrap_or_default()
            .into_iter()
            .map(|j| j.name)
            .collect();

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
