use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::{GlobalConfig, JailConfig, JailType, MountSpec};
use crate::jail::lifecycle::JailLifecycle;
use crate::jail::preset::Preset;
use crate::network::{NetworkConfig, next_free_ip};
use crate::store::DailStore;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail create myjail                              Thick jail with default base
  dail create myjail --type thin --base 14.2-RELEASE
  dail create myjail --preset postgres             Apply postgres preset
  dail create web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
  dail create app --mount /data/app:/app --allow raw_sockets --limit maxproc=256")]
pub struct CreateArgs {
    /// Jail name
    pub name: String,

    /// FreeBSD base release (e.g. 14.0-RELEASE)
    #[arg(long, add = ArgValueCompleter::new(completions::complete_base_releases))]
    pub base: Option<String>,

    /// Jail type: thick or thin
    #[arg(long, default_value = "thin", add = ArgValueCompleter::new(completions::complete_jail_type))]
    pub r#type: String,

    /// IP alias (e.g. 10.0.0.5/24)
    #[arg(long)]
    pub ip: Option<String>,

    /// Enable VNET
    #[arg(long)]
    pub vnet: bool,

    /// VNET bridge interface
    #[arg(long, default_value = "bridge0")]
    pub vnet_bridge: String,

    /// VNET IP address
    #[arg(long)]
    pub vnet_ip: Option<String>,

    /// VNET gateway
    #[arg(long)]
    pub vnet_gateway: Option<String>,

    /// nullfs mount (host:jail)
    #[arg(long = "mount")]
    pub mounts: Vec<String>,

    /// Read-only nullfs mount (host:jail)
    #[arg(long = "mount-ro")]
    pub mounts_ro: Vec<String>,

    /// Hostname
    #[arg(long)]
    pub hostname: Option<String>,

    /// Allow parameters (e.g. raw_sockets)
    #[arg(long = "allow")]
    pub allows: Vec<String>,

    /// Resource limits (e.g. maxproc=100)
    #[arg(long = "limit")]
    pub limits: Vec<String>,

    /// Keep jail alive without processes
    #[arg(long)]
    pub persist: bool,

    /// Apply a preset (e.g. postgres, nginx, dev)
    #[arg(long, add = ArgValueCompleter::new(completions::complete_preset_names))]
    pub preset: Option<String>,

    /// Network mode: inherit or none (default: auto alias from ip_pool)
    #[arg(long = "network", add = ArgValueCompleter::new(completions::complete_network_mode))]
    pub network: Option<String>,
}

pub fn run(args: CreateArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    let preset = if let Some(ref preset_name) = args.preset {
        let p = Preset::load(preset_name, &global)?
            .ok_or_else(|| anyhow::anyhow!("unknown preset: {preset_name}"))?;
        Some(p)
    } else {
        None
    };

    let jail_type = match args.r#type.as_str() {
        "thick" => JailType::Thick,
        "thin" => JailType::Thin,
        other => anyhow::bail!("invalid jail type '{other}'. Must be 'thick' or 'thin'"),
    };

    let network = if args.vnet {
        let vnet_ip = args.vnet_ip
            .ok_or_else(|| anyhow::anyhow!("--vnet requires --vnet-ip"))?;
        NetworkConfig::Vnet {
            bridge: args.vnet_bridge,
            ip: vnet_ip,
            gateway: args.vnet_gateway,
        }
    } else if let Some(ip) = args.ip {
        NetworkConfig::Alias {
            ip,
            interface: global.alias_interface.clone(),
        }
    } else {
        match args.network.as_deref() {
            Some("inherit") => NetworkConfig::Inherit,
            Some("none") => NetworkConfig::None,
            _ => {
                let store = DailStore::new(&global)?;
                let used: Vec<String> = store.list().iter().filter_map(|s| {
                    if let NetworkConfig::Alias { ip, .. } = &s.config.network {
                        Some(ip.clone())
                    } else {
                        None
                    }
                }).collect();
                let auto_ip = next_free_ip(&global.ip_pool, &used)
                    .ok_or_else(|| anyhow::anyhow!("no free IPs in pool {}", global.ip_pool))?;
                NetworkConfig::Alias {
                    ip: auto_ip,
                    interface: global.alias_interface.clone(),
                }
            }
        }
    };

    let mut mounts = Vec::new();
    for m in &args.mounts {
        let (src, dst) = m.split_once(':').ok_or_else(|| anyhow::anyhow!("invalid mount spec: {m}"))?;
        mounts.push(MountSpec {
            source: src.into(),
            destination: dst.into(),
            readonly: false,
            fs_type: "nullfs".to_string(),
        });
    }
    for m in &args.mounts_ro {
        let (src, dst) = m.split_once(':').ok_or_else(|| anyhow::anyhow!("invalid mount spec: {m}"))?;
        mounts.push(MountSpec {
            source: src.into(),
            destination: dst.into(),
            readonly: true,
            fs_type: "nullfs".to_string(),
        });
    }

    let mut params = std::collections::HashMap::new();
    let mut limits = std::collections::HashMap::new();
    let mut persist = args.persist;

    // Apply preset as base
    if let Some(ref preset) = preset {
        for (k, v) in &preset.params {
            params.insert(k.clone(), v.clone());
        }
        for (k, v) in &preset.limits {
            limits.insert(k.clone(), v.clone());
        }
        if preset.persist {
            persist = true;
        }
    }

    // Explicit args override preset
    for allow in &args.allows {
        params.insert(format!("allow.{allow}"), "true".to_string());
    }
    for limit in &args.limits {
        let (k, v) = limit.split_once('=').ok_or_else(|| anyhow::anyhow!("invalid limit: {limit}"))?;
        limits.insert(k.to_string(), v.to_string());
    }

    let config = JailConfig {
        name: args.name.clone(),
        hostname: args.hostname,
        jail_type,
        base: Some(args.base.unwrap_or(global.default_base.clone())),
        network,
        mounts,
        params,
        limits,
        persist,
        auto_remove: false,
        cmd: None,
        log_file: None,
        image_ref: None,
    };

    let state = lifecycle.create(config)?;
    println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);
    println!("Root: {}", state.root_path.display());

    Ok(())
}
