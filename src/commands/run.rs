use clap::Args;
use clap_complete::engine::ArgValueCompleter;

use crate::build::executor::BuildExecutor;
use crate::completions;
use crate::build::dailfile::Dailfile;
use crate::image::ImageStore;
use crate::jail::config::{GlobalConfig, JailConfig, JailType, MountSpec};
use crate::jail::lifecycle::JailLifecycle;
use crate::jail::preset::Preset;
use crate::network::{NetworkConfig, next_free_ip};
use crate::store::DailStore;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail run myjail                                           Create and start with defaults
  dail run postgres-jail --preset postgres                  Apply postgres preset
  dail run temp --rm                                        Auto-remove on stop
  dail run web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
  dail run app --mount /data:/app --preset dev --limit maxproc=512
  dail run postgres-jail --build ./examples/postgres/Dailfile
  dail run postgres-jail --build ./examples/postgres/Dailfile --rebuild")]
pub struct RunArgs {
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

    /// Auto-remove jail when stopped
    #[arg(long)]
    pub rm: bool,

    /// Build from Dailfile before starting
    #[arg(long)]
    pub build: Option<String>,

    /// Remove existing jail and rebuild (requires --build)
    #[arg(long, requires = "build")]
    pub rebuild: bool,

    /// Run from a saved image (name:tag)
    #[arg(long, conflicts_with_all = ["build", "base"], add = ArgValueCompleter::new(completions::complete_image_refs))]
    pub image: Option<String>,
}

pub fn run(args: RunArgs) -> anyhow::Result<()> {
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
    for m in &args.mounts_ro {
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
        let (k, v) = limit
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid limit: {limit}"))?;
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
        auto_remove: args.rm,
        cmd: None,
        log_file: None,
        image_ref: None,
    };

    if let Some(ref image_ref) = args.image {
        // --image mode: extract image into jail root
        let (img_name, img_tag) = if let Some((n, t)) = image_ref.split_once(':') {
            (n, t)
        } else {
            (image_ref.as_str(), "latest")
        };

        let image_store = ImageStore::new(&global);
        let manifest = image_store.load_manifest(img_name, img_tag)?;

        // Apply manifest config as base, CLI args override
        let mut config = config;
        for (k, v) in &manifest.params {
            config.params.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &manifest.limits {
            config.limits.entry(k.clone()).or_insert_with(|| v.clone());
        }
        if manifest.persist && !config.persist {
            config.persist = true;
        }
        if config.cmd.is_none() {
            config.cmd = manifest.cmd.clone();
        }
        config.base = manifest.base.clone();

        let jail_dir = global.jails_dir().join(&args.name);
        let root_path = jail_dir.join("root");

        if config.jail_type == JailType::Thin {
            // Thin: shared rootfs + per-jail skeleton
            let rootfs = image_store.ensure_rootfs(img_name, img_tag)?;
            config.image_ref = Some(format!("{img_name}:{img_tag}"));
            std::fs::create_dir_all(&root_path)?;
            let skeleton_dir = jail_dir.join("skeleton");
            std::fs::create_dir_all(&skeleton_dir)?;
            for dir in &["etc", "var", "tmp", "root"] {
                let src = rootfs.join(dir);
                let dst = skeleton_dir.join(dir);
                if src.exists() {
                    // Copy writable dirs from rootfs into skeleton
                    let output = std::process::Command::new("cp")
                        .args(["-a"])
                        .arg(&src)
                        .arg(&skeleton_dir)
                        .output()
                        .map_err(|e| anyhow::anyhow!("cp failed: {e}"))?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        anyhow::bail!("failed to copy {} to skeleton: {}", dir, stderr);
                    }
                } else {
                    std::fs::create_dir_all(&dst)?;
                }
            }
        } else {
            // Thick: full copy into jail root
            image_store.extract(img_name, img_tag, &root_path)?;
        }

        let state = lifecycle.create_from_image(config, root_path)?;
        println!("Jail '{}' created from image {}:{} (id: {})", state.name(), img_name, img_tag, &state.id[..8]);
    } else if let Some(ref dailfile_path) = args.build {
        // --build mode: ignore create args, use Dailfile
        if args.rebuild && lifecycle.get(&args.name).is_some() {
            lifecycle.remove(&args.name, true)?;
            println!("Jail '{}' removed for rebuild.", args.name);
        }
        let content = std::fs::read_to_string(dailfile_path)
            .map_err(|_| anyhow::anyhow!("Dailfile not found: {dailfile_path}"))?;
        let dailfile = Dailfile::parse(&content)?;
        let context_dir = std::path::Path::new(dailfile_path.as_str())
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        BuildExecutor::build(&mut lifecycle, &dailfile, &args.name, Some(&config), &context_dir)?;
        println!("Jail '{}' built.", args.name);
    } else {
        let state = lifecycle.create(config)?;
        println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);
    }

    if let Err(e) = lifecycle.start(&args.name) {
        tracing::warn!("start failed, cleaning up jail '{}'", args.name);
        let _ = lifecycle.remove(&args.name, false);
        return Err(e.into());
    }
    println!("Jail '{}' started.", args.name);

    if args.rm {
        println!("(will be auto-removed on stop)");
    }

    Ok(())
}
