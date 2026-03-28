use crate::jail::config::{GlobalConfig, JailConfig, JailType, MountSpec, PortMapping};
use crate::jail::preset::Preset;
use crate::network::{NetworkConfig, next_free_ip};
use crate::storage;
use crate::store::DailStore;

/// Common arguments shared between create and run commands
pub struct CommonJailArgs<'a> {
    pub base: Option<&'a str>,
    pub r#type: &'a str,
    pub ip: Option<&'a str>,
    pub vnet: bool,
    pub vnet_bridge: &'a str,
    pub vnet_ip: Option<&'a str>,
    pub vnet_gateway: Option<&'a str>,
    pub mounts: &'a [String],
    pub mounts_ro: &'a [String],
    pub hostname: Option<&'a str>,
    pub allows: &'a [String],
    pub limits: &'a [String],
    pub no_persist: bool,
    pub preset: Option<&'a str>,
    pub network: Option<&'a str>,
    pub publish: &'a [String],
    pub envs: &'a [String],
    pub uid: Option<u32>,
}

/// Additional flags for JailConfig that differ between create and run
#[derive(Default)]
pub struct ConfigFlags {
    pub auto_remove: bool,
    pub cmd: Option<String>,
    pub log_file: Option<String>,
}

/// Information about resources allocated during jail config building
pub struct AllocationInfo {
    pub allocated_ip: Option<String>,
    pub base_release: String,
}

/// Validate that base release is bootstrapped, or warn user if not.
/// This is non-fatal since base will be auto-downloaded on first use.
fn validate_or_warn_base(base_name: &str, global: &GlobalConfig) {
    let backend = storage::get_backend(global);
    match backend.list_bases(global) {
        Ok(bases) => {
            if !bases.iter().any(|(name, _)| name == base_name || name.starts_with(&format!("{base_name}-"))) {
                eprintln!("⚠ Warning: base '{}' not bootstrapped yet.", base_name);
                if !bases.is_empty() {
                    let available: Vec<&str> = bases.iter().map(|(n, _)| n.as_str()).collect();
                    eprintln!("Available: {}", available.join(", "));
                }
                eprintln!("It will be downloaded automatically on first use.");
            }
        }
        Err(_) => {
            // Silently ignore - maybe root_dir doesn't exist yet but will be created
        }
    }
}

/// Check if an explicit IP conflicts with existing jails.
/// Returns error if the IP is already in use.
fn check_ip_conflict(ip: &str, global: &GlobalConfig) -> anyhow::Result<()> {
    let store = match DailStore::new(global) {
        Ok(s) => s,
        Err(_) => return Ok(()), // Store not initialized yet, no conflicts possible
    };

    for jail in store.list() {
        if let NetworkConfig::Alias {
            ip: existing_ip, ..
        } = &jail.config.network
            && existing_ip == ip
        {
            anyhow::bail!(
                "IP {} is already used by jail '{}'. Use `dail ls` to see all allocations.",
                ip,
                jail.config.name
            );
        }
    }
    Ok(())
}

/// Build a JailConfig from common arguments, handling presets, network allocation, and validation.
/// Returns the config and information about allocated resources for user feedback.
pub fn build_jail_config(
    name: String,
    common: &CommonJailArgs,
    global: &GlobalConfig,
    flags: ConfigFlags,
) -> anyhow::Result<(JailConfig, AllocationInfo)> {
    tracing::info!(
        "build_jail_config called for jail '{}' (type: {})",
        &name,
        common.r#type
    );
    // Load and apply preset if specified
    let preset = if let Some(preset_name) = common.preset {
        let p = Preset::load(preset_name, global)?
            .ok_or_else(|| anyhow::anyhow!("unknown preset: {preset_name}"))?;
        Some(p)
    } else {
        None
    };

    // Parse jail type
    let jail_type = match common.r#type {
        "thick" => JailType::Thick,
        "thin" => JailType::Thin,
        other => anyhow::bail!("invalid jail type '{other}'. Must be 'thick' or 'thin'"),
    };

    // Configure network with optional auto-allocation
    let mut allocated_ip = None;
    let network = if common.vnet {
        let vnet_ip = common.vnet_ip.ok_or_else(|| {
            anyhow::anyhow!(
                "--vnet requires --vnet-ip (e.g., 10.0.0.5/24) and optionally --vnet-gateway"
            )
        })?;
        NetworkConfig::Vnet {
            bridge: common.vnet_bridge.to_string(),
            ip: vnet_ip.to_string(),
            gateway: common.vnet_gateway.map(|s| s.to_string()),
        }
    } else if let Some(ip) = common.ip {
        // Check for IP conflicts before creating the jail
        check_ip_conflict(ip, global)?;
        NetworkConfig::Alias {
            ip: ip.to_string(),
            interface: global.alias_interface.clone(),
        }
    } else {
        match common.network {
            Some("inherit") => NetworkConfig::Inherit,
            Some("none") => NetworkConfig::None,
            _ => {
                // Auto-allocate IP from pool
                let store = DailStore::new_readonly(global)?;
                let used: Vec<String> = store
                    .list()
                    .iter()
                    .filter_map(|s| {
                        if let NetworkConfig::Alias { ip, .. } = &s.config.network {
                            Some(ip.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                let auto_ip = next_free_ip(&global.ip_pool, &used)
                    .ok_or_else(|| anyhow::anyhow!("no free IPs in pool {}", global.ip_pool))?;
                allocated_ip = Some(auto_ip.clone());
                NetworkConfig::Alias {
                    ip: auto_ip,
                    interface: global.alias_interface.clone(),
                }
            }
        }
    };

    // Parse mounts
    let mut mounts = Vec::new();
    for m in common.mounts {
        let (src, dst) = m
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid mount spec: {m}"))?;
        mounts.push(MountSpec {
            source: src.into(),
            destination: dst.into(),
            readonly: false,
            fs_type: "nullfs".to_string(),
        });
    }
    for m in common.mounts_ro {
        let (src, dst) = m
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid mount spec: {m}"))?;
        mounts.push(MountSpec {
            source: src.into(),
            destination: dst.into(),
            readonly: true,
            fs_type: "nullfs".to_string(),
        });
    }

    // Build params and limits, applying preset as base
    let mut params = std::collections::HashMap::new();
    let mut limits = std::collections::HashMap::new();
    let persist = !common.no_persist;

    // Apply preset values as base
    if let Some(ref preset) = preset {
        for (k, v) in &preset.params {
            params.insert(k.clone(), v.clone());
        }
        for (k, v) in &preset.limits {
            limits.insert(k.clone(), v.clone());
        }
    }

    // Explicit CLI args override preset
    for allow in common.allows {
        params.insert(format!("allow.{allow}"), "true".to_string());
    }
    for limit in common.limits {
        let (k, v) = limit
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid limit: {limit}"))?;
        limits.insert(k.to_string(), v.to_string());
    }

    let base_release = common
        .base
        .map(|s| s.to_string())
        .unwrap_or_else(|| global.default_base.clone());

    // Warn if base is not bootstrapped (non-fatal, will auto-download)
    validate_or_warn_base(&base_release, global);

    let mut ports = Vec::new();
    for p in common.publish {
        ports.push(PortMapping::parse(p)?);
    }

    // Parse env vars
    let mut env = std::collections::HashMap::new();
    for e in common.envs {
        let (k, v) = e
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid env spec: {e}"))?;
        env.insert(k.to_string(), v.to_string());
    }

    let config = JailConfig {
        name,
        hostname: common.hostname.map(|s| s.to_string()),
        jail_type,
        base: Some(base_release.clone()),
        network,
        mounts,
        params,
        limits,
        persist,
        auto_remove: flags.auto_remove,
        cmd: flags.cmd,
        log_file: flags.log_file,
        ports,
        env,
        service_uid: common.uid,
    };

    let info = AllocationInfo {
        allocated_ip,
        base_release,
    };

    Ok((config, info))
}
