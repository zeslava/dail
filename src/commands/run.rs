use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use uuid::Uuid;

use crate::build::dailfile::Dailfile;
use crate::build::executor::BuildExecutor;
use crate::commands::shared::{self, CommonJailArgs, ConfigFlags};
use crate::completions;
use crate::image::{ImageRef, ImageStore};
use crate::jail::config::{GlobalConfig, JailType};
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail run                                                  Auto-detect Dailfile in current dir
  dail run myjail                                           Create and start with defaults
  dail run postgres-jail --preset postgres                  Apply postgres preset
  dail run temp --rm                                        Auto-remove on stop
  dail run web --vnet --vnet-ip 10.0.0.5/24 --vnet-gateway 10.0.0.1
  dail run app --mount /data:/app --preset dev --limit maxproc=512
  dail run Dailfile                                         Build from ./Dailfile in current dir
  dail run ./examples/postgres/Dailfile                     Build from specific Dailfile
  dail run postgres-jail --build ./examples/postgres/Dailfile
  dail run postgres-jail --build ./examples/postgres/Dailfile --rebuild")]
pub struct RunArgs {
    /// Jail name or Dailfile path (optional, auto-detects ./Dailfile if not provided)
    pub name: Option<String>,

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

    /// Network mode: 'inherit' (host network), 'none' (isolated), or omit for auto IP from pool
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

pub fn run(mut args: RunArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    // Auto-detect: if no name provided, look for Dailfile in current dir
    let name_or_path = match args.name {
        Some(n) => n,
        None => {
            if std::path::Path::new("Dailfile").exists() {
                "Dailfile".to_string()
            } else if std::path::Path::new("dailfile").exists() {
                "dailfile".to_string()
            } else {
                anyhow::bail!(
                    "No jail name provided and no Dailfile found in current directory.\nUsage: dail run <jail-name> [OPTIONS] or dail run [OPTIONS]"
                );
            }
        }
    };

    // Auto-detect Dailfile mode: if name looks like a file path (contains / or is named Dailfile)
    if args.build.is_none()
        && (name_or_path.ends_with(".dailfile")
            || name_or_path == "Dailfile"
            || name_or_path == "dailfile"
            || name_or_path.contains('/'))
    {
        // Treat as Dailfile path, use jail name as generated
        args.build = Some(name_or_path);
        args.name = Some(format!(
            "dail-build-{}",
            Uuid::new_v4().to_string()[..8].to_string()
        ));
    } else {
        args.name = Some(name_or_path);
    }

    // Unwrap: name is guaranteed to be Some after above logic
    let jail_name = args.name.unwrap();

    let common = CommonJailArgs {
        base: args.base.as_deref(),
        r#type: &args.r#type,
        ip: args.ip.as_deref(),
        vnet: args.vnet,
        vnet_bridge: &args.vnet_bridge,
        vnet_ip: args.vnet_ip.as_deref(),
        vnet_gateway: args.vnet_gateway.as_deref(),
        mounts: &args.mounts,
        mounts_ro: &args.mounts_ro,
        hostname: args.hostname.as_deref(),
        allows: &args.allows,
        limits: &args.limits,
        persist: args.persist,
        preset: args.preset.as_deref(),
        network: args.network.as_deref(),
    };

    let flags = ConfigFlags {
        auto_remove: args.rm,
        ..Default::default()
    };

    let (config, info) = shared::build_jail_config(jail_name.clone(), &common, &global, flags)?;

    if let Some(ref image_ref_str) = args.image {
        // --image mode: extract image into jail root
        let image_ref = ImageRef::parse(image_ref_str)?;
        let img_name = &image_ref.name;
        let img_tag = &image_ref.tag;

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

        let jail_dir = global.jails_dir().join(&jail_name);
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
        println!(
            "Jail '{}' created from image {}:{} (id: {})",
            state.name(),
            img_name,
            img_tag,
            &state.id[..8]
        );

        if let Some(ip) = info.allocated_ip {
            println!("IP allocated: {} on {}", ip, global.alias_interface);
        }
    } else if let Some(ref dailfile_path) = args.build {
        // --build mode: ignore create args, use Dailfile
        if let Some(existing) = lifecycle.get(&jail_name) {
            if args.rebuild {
                lifecycle.remove(&jail_name, true)?;
                println!("Jail '{}' removed for rebuild.", jail_name);
            } else if existing.is_stopped() {
                // Auto-remove stopped jail (e.g. after reconcile or previous build)
                lifecycle.remove(&jail_name, false)?;
                println!("Jail '{}' (stopped) removed for rebuild.", jail_name);
            } else {
                anyhow::bail!(
                    "Jail '{}' already exists and is running. Use --rebuild to force rebuild.",
                    jail_name
                );
            }
        }
        let content = std::fs::read_to_string(dailfile_path)
            .map_err(|_| anyhow::anyhow!("Dailfile not found: {dailfile_path}"))?;
        let dailfile = Dailfile::parse(&content)?;
        let context_dir = std::path::Path::new(dailfile_path.as_str())
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        BuildExecutor::build(
            &mut lifecycle,
            &dailfile,
            &jail_name,
            Some(&config),
            &context_dir,
        )?;
        println!("Jail '{}' built.", jail_name);

        if let Some(ip) = info.allocated_ip {
            println!("IP allocated: {} on {}", ip, global.alias_interface);
        }
    } else {
        let state = lifecycle.create(config)?;
        println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);

        if let Some(ip) = info.allocated_ip {
            println!("IP allocated: {} on {}", ip, global.alias_interface);
        }

        println!("Base: {}", info.base_release);
    }

    if let Err(e) = lifecycle.start(&jail_name) {
        tracing::warn!("start failed, cleaning up jail '{}'", jail_name);
        let _ = lifecycle.remove(&jail_name, false);
        return Err(e.into());
    }
    println!("Jail '{}' started.", jail_name);

    if args.rm {
        println!("(will be auto-removed on stop)");
    }

    Ok(())
}
