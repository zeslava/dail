use crate::error::DailError;
use crate::freebsd::jail_sys::{JailParams, JailSys};
use crate::freebsd::mount::NullfsMount;
use crate::jail::config::{GlobalConfig, JailConfig, JailType, StorageKind};
use crate::jail::state::{JailState, JailStatus};
use crate::network::{self, NetworkConfig};
use crate::storage;
use crate::store::DailStore;

pub struct JailLifecycle {
    config: GlobalConfig,
    store: DailStore,
}

/// Validate jail name: must start with letter, then letters/digits/underscore/hyphen.
fn validate_jail_name(name: &str) -> Result<(), DailError> {
    if name.is_empty() {
        return Err(DailError::Config("jail name cannot be empty".to_string()));
    }
    let mut chars = name.chars();
    // SAFETY: checked name.is_empty() above
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(DailError::Config(format!(
            "jail name must start with a letter, got '{name}'"
        )));
    }
    for ch in chars {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' {
            return Err(DailError::Config(format!(
                "jail name contains invalid character '{ch}'. Allowed: [a-zA-Z0-9_-]"
            )));
        }
    }
    Ok(())
}

impl JailLifecycle {
    pub fn new(config: GlobalConfig) -> Result<Self, DailError> {
        let store = DailStore::new(&config)?;
        Ok(Self { config, store })
    }

    /// Create a lifecycle instance in read-only mode (shared lock, no reconcile).
    /// Suitable for ls, inspect, logs commands.
    pub fn new_readonly(config: GlobalConfig) -> Result<Self, DailError> {
        let store = DailStore::new_readonly(&config)?;
        Ok(Self { config, store })
    }

    pub fn global_config(&self) -> &GlobalConfig {
        &self.config
    }

    pub fn create(&mut self, jail_config: JailConfig) -> Result<JailState, DailError> {
        validate_jail_name(&jail_config.name)?;

        if self.store.get(&jail_config.name).is_some() {
            return Err(DailError::JailAlreadyExists(jail_config.name.clone()));
        }

        let backend = storage::get_backend(&self.config);

        // Log base release if specified
        if let Some(ref base) = jail_config.base {
            tracing::info!("Preparing base system: {}", base);
        }

        tracing::info!("Creating jail root for '{}'", jail_config.name);
        let root_path = backend.create_jail_root(&self.config, &jail_config)?;
        tracing::info!("Jail root created at {}", root_path.display());

        let state = JailState::new(jail_config, root_path);
        self.store.add(state.clone())?;
        Ok(state)
    }

    pub fn start(&mut self, name: &str) -> Result<&JailState, DailError> {
        let state = self.validate_and_clone_for_start(name)?;

        match self.start_inner(&state) {
            Ok((jid, epair)) => {
                self.store.update(name, |s| {
                    s.status = JailStatus::Running;
                    s.jid = Some(jid);
                    s.epair = epair;
                    s.started_at = Some(chrono::Utc::now());
                })?;
                if let Some(ref cmd) = state.config.cmd {
                    let log_path = self.cmd_log_path(&state);
                    if let Err(e) = JailSys::exec_logged(&state.config.name, cmd, &log_path, &state.config.env) {
                        tracing::error!("Failed to launch CMD for jail '{}': {}", state.name(), e);
                        self.cleanup_failed_start(&state);
                        return Err(DailError::Other(e.to_string()));
                    }
                }
                // SAFETY: just updated via store.update() above
                Ok(self.store.get(name).unwrap())
            }
            Err(e) => {
                tracing::error!("Failed to start jail '{}': {}", state.name(), e);
                tracing::debug!(
                    "Attempting cleanup after failed start for jail '{}'",
                    state.name()
                );
                self.cleanup_failed_start(&state);
                Err(e)
            }
        }
    }

    /// Start the jail and run CMD in foreground with inherited stdout/stderr.
    /// Blocks until CMD exits and returns its exit code (128+N for signals, -1 if unknown).
    pub fn start_foreground(&mut self, name: &str) -> Result<i32, DailError> {
        let state = self.validate_and_clone_for_start(name)?;

        match self.start_inner(&state) {
            Ok((jid, epair)) => {
                self.store.update(name, |s| {
                    s.status = JailStatus::Running;
                    s.jid = Some(jid);
                    s.epair = epair;
                    s.started_at = Some(chrono::Utc::now());
                })?;
                if let Some(ref cmd) = state.config.cmd {
                    let status = JailSys::exec_foreground(
                        &state.config.name,
                        cmd,
                        &state.config.env,
                    )?;
                    return Ok(exit_code_from_status(status));
                }
                Ok(0)
            }
            Err(e) => {
                tracing::error!("Failed to start jail '{}': {}", state.name(), e);
                self.cleanup_failed_start(&state);
                Err(e)
            }
        }
    }

    fn validate_and_clone_for_start(&self, name: &str) -> Result<JailState, DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        if state.status == JailStatus::Running {
            return Err(DailError::InvalidState {
                name: name.to_string(),
                status: "already running".to_string(),
                expected: format!("stop it first with `dail stop {name}`"),
            });
        }

        if !state.root_path.exists() {
            return Err(DailError::Storage(format!(
                "jail root path does not exist: {}",
                state.root_path.display()
            )));
        }
        for mount in &state.config.mounts {
            if !mount.source.exists() {
                return Err(DailError::Storage(format!(
                    "mount source does not exist: {}",
                    mount.source.display()
                )));
            }
        }

        Ok(state.clone())
    }

    fn cmd_log_path(&self, state: &JailState) -> std::path::PathBuf {
        if let Some(ref log_file) = state.config.log_file {
            let rel = log_file.strip_prefix('/').unwrap_or(log_file);
            state.root_path.join(rel)
        } else {
            self.config.jails_dir().join(state.name()).join("cmd.log")
        }
    }

    /// Start a jail with temporary config overrides for the build phase.
    /// The overrides are applied in-memory only — persisted state is never mutated.
    pub fn start_for_build(&mut self, name: &str) -> Result<&JailState, DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        let mut build_state = state.clone();
        build_state.config.persist = true;
        build_state.config.network = crate::network::NetworkConfig::Inherit;
        build_state.config.cmd = None;

        match self.start_inner(&build_state) {
            Ok((jid, epair)) => {
                self.store.update(name, |s| {
                    s.status = JailStatus::Running;
                    s.jid = Some(jid);
                    s.epair = epair;
                    s.started_at = Some(chrono::Utc::now());
                })?;
                Ok(self.store.get(name).unwrap())
            }
            Err(e) => {
                self.cleanup_failed_start(&build_state);
                Err(e)
            }
        }
    }

    /// Inner start logic that can fail at any point. On error, caller handles cleanup.
    /// Returns (jid, epair_name) where epair_name is set for Vnet jails.
    fn start_inner(&self, state: &JailState) -> Result<(i32, Option<String>), DailError> {
        // 1. Mount thin jail base + skeleton if applicable
        if state.config.jail_type == JailType::Thin {
            if let Some(ref release) = state.config.base {
                let backend = storage::get_backend(&self.config);
                backend.check_base(&self.config, release)?;
            } else {
                return Err(DailError::Config(
                    "thin jail requires a base release. Run 'dail bootstrap <release>' first, or use --type thick".to_string(),
                ));
            }

            // Directory backend: mount base read-only, then skeleton writable on top
            // ZFS backend: clone is already writable, no mounts needed
            if self.config.storage_backend != StorageKind::Zfs {
                let base_dir = {
                    let backend = storage::get_backend(&self.config);
                    backend.check_base(&self.config, state.config.base.as_deref().unwrap())?
                };
                NullfsMount::mount(&base_dir, &state.root_path, true)?;

                let jail_dir = self.config.jails_dir().join(state.name());
                let skeleton_dir = jail_dir.join("skeleton");
                for dir in &["etc", "var", "tmp", "root"] {
                    let src = skeleton_dir.join(dir);
                    let dst = state.root_path.join(dir);
                    std::fs::create_dir_all(&src)?;
                    std::fs::create_dir_all(&dst)?;
                    NullfsMount::mount(&src, &dst, false)?;
                }
            }
        }

        // 2. Mount devfs
        NullfsMount::mount_devfs(&state.root_path)?;

        // 3. Mount nullfs volumes
        for mount in &state.config.mounts {
            let target = state.root_path.join(
                mount
                    .destination
                    .strip_prefix("/")
                    .unwrap_or(&mount.destination),
            );
            std::fs::create_dir_all(&target)?;
            NullfsMount::mount(&mount.source, &target, mount.readonly)?;
        }

        // 4. Build jail params and create jail
        let mut extra = state.config.params.clone();
        let ip4_addr = match &state.config.network {
            NetworkConfig::Alias { ip, .. } => Some(ip.clone()),
            NetworkConfig::Inherit => {
                extra.insert("ip4".to_string(), "inherit".to_string());
                None
            }
            NetworkConfig::None => {
                extra.insert("ip4".to_string(), "disable".to_string());
                extra.insert("ip6".to_string(), "disable".to_string());
                None
            }
            NetworkConfig::Vnet { .. } => None,
        };

        let vnet = matches!(&state.config.network, NetworkConfig::Vnet { .. });

        // FreeBSD requires persist or exec.start; since CMD runs via jexec
        // after jail creation, always enable persist to keep the jail alive
        let persist = true;

        let params = JailParams {
            name: state.config.name.clone(),
            path: state.root_path.clone(),
            hostname: state.config.hostname.clone(),
            ip4_addr,
            vnet,
            persist,
            extra,
        };

        let jid = JailSys::create(&params)?;

        // 5. Network setup
        let epair = network::setup_network(&state.config.name, &state.config.network)?;

        // 5b. Port forwarding via PF
        if let NetworkConfig::Alias { ref ip, .. } = state.config.network {
            if !state.config.ports.is_empty() {
                crate::freebsd::pf::setup_port_forwarding(
                    &state.config.name,
                    ip,
                    &state.config.ports,
                );
            }
        }

        // 6. Resource limits
        for (resource, value) in &state.config.limits {
            crate::freebsd::rctl::Rctl::add_limit(&state.config.name, resource, value)
                .map_err(|e| DailError::Other(e.to_string()))?;
        }

        // 7. Create log file if LOG is configured
        if let Some(ref log_file) = state.config.log_file {
            use std::os::unix::fs::PermissionsExt;
            let rel = log_file.strip_prefix('/').unwrap_or(log_file);
            let log_path = state.root_path.join(rel);
            if let Some(parent) = log_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)?;
            std::fs::set_permissions(&log_path, std::fs::Permissions::from_mode(0o666))?;
        }

        // 8. Write env file for rc.d scripts
        if !state.config.env.is_empty() {
            let env_dir = state.root_path.join("usr/local/etc/dail");
            std::fs::create_dir_all(&env_dir)?;
            let env_path = env_dir.join(format!("{}.env", state.config.name));
            let content: String = state
                .config
                .env
                .iter()
                .map(|(k, v)| format!("export {k}=\"{v}\""))
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&env_path, content + "\n")?;
        }

        Ok((jid, epair))
    }

    /// Best-effort cleanup after a failed start: unmount nullfs, devfs, thin jail mounts, remove jail.
    fn cleanup_failed_start(&self, state: &JailState) {
        tracing::info!("Cleaning up failed start for jail '{}'", state.name());

        // Unmount nullfs in reverse order
        for mount in state.config.mounts.iter().rev() {
            let target = state.root_path.join(
                mount
                    .destination
                    .strip_prefix("/")
                    .unwrap_or(&mount.destination),
            );
            if let Err(e) = NullfsMount::force_unmount(&target) {
                tracing::debug!("Failed to unmount {}: {}", target.display(), e);
            }
        }

        // Unmount devfs
        let devfs = state.root_path.join("dev");
        if devfs.exists() {
            if let Err(e) = NullfsMount::force_unmount(&devfs) {
                tracing::debug!("Failed to unmount devfs: {}", e);
            }
        }

        // Unmount thin jail skeleton + base (directory backend only)
        if state.config.jail_type == JailType::Thin && self.config.storage_backend != StorageKind::Zfs {
            for dir in &["root", "tmp", "var", "etc"] {
                let dst = state.root_path.join(dir);
                if let Err(e) = NullfsMount::force_unmount(&dst) {
                    tracing::debug!("Failed to unmount {}: {}", dst.display(), e);
                }
            }
            if let Err(e) = NullfsMount::force_unmount(&state.root_path) {
                tracing::debug!("Failed to unmount jail root: {}", e);
            }
        }

        // Try to remove jail (may not have been created yet)
        match JailSys::remove(&state.config.name) {
            Ok(()) => {
                tracing::info!(
                    "Successfully removed jail '{}' during cleanup",
                    state.config.name
                );
            }
            Err(e) => {
                tracing::debug!("Failed to remove jail '{}': {}", state.config.name, e);
            }
        }
    }

    pub fn stop(&mut self, name: &str) -> Result<(), DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        if state.status != JailStatus::Running && state.status != JailStatus::Idle {
            return Err(DailError::InvalidState {
                name: name.to_string(),
                status: format!("{}, not running", state.status),
                expected: format!("start it first with `dail start {name}`"),
            });
        }

        let network = state.config.network.clone();
        let root_path = state.root_path.clone();
        let mounts = state.config.mounts.clone();
        let epair = state.epair.clone();
        let jail_type = state.config.jail_type;
        let has_ports = !state.config.ports.is_empty();

        if has_ports {
            crate::freebsd::pf::teardown_port_forwarding(name);
        }

        network::teardown_network(name, &network, epair.as_deref())?;
        let _ = crate::freebsd::rctl::Rctl::remove_limits(name);
        // Jail may already be gone (crashed, host reboot)
        let _ = JailSys::remove(name);

        // Unmount nullfs in reverse order
        for mount in mounts.iter().rev() {
            let target = root_path.join(
                mount
                    .destination
                    .strip_prefix("/")
                    .unwrap_or(&mount.destination),
            );
            let _ = NullfsMount::force_unmount(&target);
        }

        // Unmount devfs
        let devfs = root_path.join("dev");
        if devfs.exists() {
            let _ = NullfsMount::force_unmount(&devfs);
        }

        // Unmount thin jail skeleton + base (directory backend only)
        if jail_type == JailType::Thin && self.config.storage_backend != StorageKind::Zfs {
            for dir in &["root", "tmp", "var", "etc"] {
                let _ = NullfsMount::force_unmount(&root_path.join(dir));
            }
            let _ = NullfsMount::force_unmount(&root_path);
        }

        let auto_remove = self
            .store
            .get(name)
            .map(|s| s.config.auto_remove)
            .unwrap_or(false);

        self.store.update(name, |s| {
            s.status = JailStatus::Stopped;
            s.jid = None;
            s.epair = None;
            s.stopped_at = Some(chrono::Utc::now());
        })?;

        if auto_remove {
            self.remove(name, false)?;
        }

        Ok(())
    }

    pub fn remove(&mut self, name: &str, force: bool) -> Result<(), DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        if state.status == JailStatus::Running || state.status == JailStatus::Idle {
            if force {
                self.stop(name)?;
            } else {
                return Err(DailError::InvalidState {
                    name: name.to_string(),
                    status: "running".to_string(),
                    expected: format!("stop it first or use `dail rm --force {name}`"),
                });
            }
        }

        // SAFETY: checked existence on line above; stop() doesn't remove from store
        let state = self.store.get(name).unwrap().clone();

        // Best-effort: remove jail from kernel if it lingers (e.g. persist without processes)
        let _ = JailSys::remove(name);

        let backend = storage::get_backend(&self.config);
        backend.destroy(&self.config, &state)?;

        self.store.remove(name)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<&JailState> {
        self.store.list()
    }

    pub fn get(&self, name: &str) -> Option<&JailState> {
        self.store.get(name)
    }

    pub fn exec(
        &self,
        name: &str,
        command: &[&str],
    ) -> Result<std::process::ExitStatus, DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        if state.status != JailStatus::Running && state.status != JailStatus::Idle {
            return Err(DailError::InvalidState {
                name: name.to_string(),
                status: format!("{}, not running", state.status),
                expected: format!("start it first with `dail start {name}`"),
            });
        }

        Ok(JailSys::exec(name, command)?)
    }

    pub fn console(&self, name: &str, shell: &str) -> Result<std::process::ExitStatus, DailError> {
        let state = self
            .store
            .get(name)
            .ok_or_else(|| DailError::JailNotFound(name.to_string()))?;

        if state.status != JailStatus::Running && state.status != JailStatus::Idle {
            return Err(DailError::InvalidState {
                name: name.to_string(),
                status: format!("{}, not running", state.status),
                expected: format!("start it first with `dail start {name}`"),
            });
        }

        Ok(JailSys::console(name, shell)?)
    }
}

fn exit_code_from_status(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    if let Some(code) = status.code() {
        code
    } else if let Some(signal) = status.signal() {
        128 + signal
    } else {
        -1
    }
}
