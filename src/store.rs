use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use crate::error::DailError;
use crate::freebsd::jail_sys::{JailInfo, JailSys};
use crate::freebsd::ps;
use crate::jail::config::{GlobalConfig, JailConfig};
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

        store.hydrate_status();
        Ok(store)
    }

    /// Open the store in read-only mode.
    /// No locks needed—just read the state.json file directly.
    /// Safe for concurrent reads, and avoids deadlock issues entirely.
    pub fn new_readonly(config: &GlobalConfig) -> Result<Self, DailError> {
        let state_path = config.state_path();
        let jails_dir = config.jails_dir();

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

        let mut store = Self {
            jails,
            state_path,
            jails_dir,
        };
        store.hydrate_status();
        Ok(store)
    }

    /// Compute status and jid from kernel state (`jls`).
    /// Status is never persisted — always derived from `started_at` + `jls`.
    /// Also discovers orphan jails (in kernel with meta=dail but not in store).
    fn hydrate_status(&mut self) {
        let kernel_jails: Vec<JailInfo> = JailSys::list().unwrap_or_default();
        let mut running_jails: HashMap<String, &JailInfo> = kernel_jails
            .iter()
            .map(|j| (j.name.clone(), j))
            .collect();

        for jail in &mut self.jails {
            if let Some(info) = running_jails.remove(&jail.config.name) {
                jail.jid = Some(info.jid);
                jail.status = if ps::jail_has_processes(info.jid) {
                    JailStatus::Running
                } else {
                    JailStatus::Idle
                };
            } else if jail.started_at.is_some() {
                jail.status = JailStatus::Stopped;
                jail.jid = None;
            }
            // else: started_at is None → default Created
        }

        // Remaining kernel jails with meta=dail are orphans
        for (_, info) in running_jails {
            if info.meta != "dail" {
                continue;
            }
            let status = if ps::jail_has_processes(info.jid) {
                JailStatus::Running
            } else {
                JailStatus::Idle
            };
            let config = JailConfig {
                name: info.name.clone(),
                hostname: Some(info.hostname.clone()),
                ..Default::default()
            };
            let mut state = JailState::new(config, PathBuf::from(&info.path));
            state.jid = Some(info.jid);
            state.status = status;
            self.jails.push(state);
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

        // Atomic replacement must retain access granted to existing readers,
        // regardless of the writer's umask. New indexes contain secrets in env.
        let permissions = match std::fs::metadata(&self.state_path) {
            Ok(metadata) => metadata.permissions(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::Permissions::from_mode(0o600)
            }
            Err(e) => return Err(e.into()),
        };

        let tmp_path = self.state_path.with_extension("json.tmp");
        let mut tmp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)?;
        tmp_file.set_permissions(permissions)?;
        tmp_file.write_all(content.as_bytes())?;
        tmp_file.sync_all()?;

        std::fs::rename(&tmp_path, &self.state_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_store(state_path: PathBuf) -> DailStore {
        DailStore {
            jails: Vec::new(),
            state_path,
            jails_dir: PathBuf::new(),
        }
    }

    #[test]
    fn save_index_preserves_permissions_under_restrictive_umask() {
        const CHILD_PATH: &str = "DAIL_TEST_STATE_PERMISSIONS_PATH";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            // Only this dedicated subprocess changes its process-wide umask.
            unsafe {
                libc::umask(0o077);
            }
            let store = empty_store(PathBuf::from(path));
            for _ in 0..2 {
                store.save_index().expect("save index");
                let permissions = std::fs::metadata(&store.state_path)
                    .expect("read permissions")
                    .permissions();
                assert_eq!(permissions.mode() & 0o777, 0o640);
                assert_eq!(
                    std::fs::read_to_string(&store.state_path).expect("read index"),
                    "[]"
                );
            }
            return;
        }

        let directory = tempfile::tempdir().expect("create test directory");
        let path = directory.path().join("state.json");
        std::fs::write(&path, "[]").expect("create index");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("grant group read access");
        let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "store::tests::save_index_preserves_permissions_under_restrictive_umask",
            ])
            .env(CHILD_PATH, &path)
            .status()
            .expect("run restrictive-umask child");
        assert!(status.success());
    }

    #[test]
    fn new_index_is_private() {
        let directory = tempfile::tempdir().expect("create test directory");
        let store = empty_store(directory.path().join("state.json"));
        store.save_index().expect("save index");
        assert_eq!(
            std::fs::metadata(&store.state_path)
                .expect("index metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
