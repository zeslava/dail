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
  dail run pg                                               Create and start with defaults
  dail run pg --preset postgres                             Apply postgres preset
  dail run pg --rm                                          Auto-remove on stop
  dail run ./examples/postgres/Dailfile pg                  Build Dailfile, name jail 'pg'
  dail run ./examples/postgres/Dailfile                     Build with auto-generated name
  dail run                                                  Auto-detect ./Dailfile
  dail run ./examples/postgres/Dailfile pg --rebuild        Rebuild from scratch")]
pub struct RunArgs {
    /// Jail name or Dailfile path
    pub first: Option<String>,

    /// Jail name (when first argument is a Dailfile path)
    pub second: Option<String>,

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

/// Find Dailfile by pattern (*.Dailfile or *.dailfile)
fn find_dailfile_pattern() -> Option<String> {
    use std::fs;

    if let Ok(entries) = fs::read_dir(".") {
        let mut dailfiles: Vec<_> = entries
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    let path = e.path();
                    let filename = path.file_name()?;
                    let name = filename.to_str()?;

                    // Match *.Dailfile or *.dailfile
                    if (name.ends_with(".Dailfile") || name.ends_with(".dailfile"))
                        && path.is_file()
                    {
                        Some(name.to_string())
                    } else {
                        None
                    }
                })
            })
            .collect();

        // Sort for consistent behavior
        dailfiles.sort();

        // Return first match
        dailfiles.into_iter().next()
    } else {
        None
    }
}

pub fn run(mut args: RunArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    // Resolve first/second positional args into (jail_name, optional dailfile_path)
    let first = match args.first {
        Some(f) => f,
        None => {
            // No args: auto-detect Dailfile in current dir
            if std::path::Path::new("Dailfile").exists() {
                "Dailfile".to_string()
            } else if std::path::Path::new("dailfile").exists() {
                "dailfile".to_string()
            } else if let Some(path) = find_dailfile_pattern() {
                path
            } else {
                anyhow::bail!(
                    "No arguments and no Dailfile found.\nUsage: dail run <name> or dail run <Dailfile> [name]"
                );
            }
        }
    };

    fn looks_like_dailfile(s: &str) -> bool {
        s.ends_with(".Dailfile")
            || s.ends_with(".dailfile")
            || s == "Dailfile"
            || s == "dailfile"
            || s.contains('/')
    }

    let jail_name;
    if args.build.is_none() && looks_like_dailfile(&first) {
        // first arg is a Dailfile path, second (if any) is the jail name
        jail_name = args.second.unwrap_or_else(|| {
            format!("dail-build-{}", &Uuid::new_v4().to_string()[..8])
        });
        args.build = Some(first);
    } else {
        // first arg is the jail name
        if args.second.is_some() {
            anyhow::bail!("unexpected second argument. Did you mean: dail run <Dailfile> <name>?");
        }
        jail_name = first;
    }

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
        tracing::info!("Reading Dailfile from {}", dailfile_path);
        let content = std::fs::read_to_string(dailfile_path)
            .map_err(|_| anyhow::anyhow!("Dailfile not found: {dailfile_path}"))?;
        tracing::info!("Dailfile read successfully ({} bytes)", content.len());
        tracing::info!("Calling Dailfile::parse...");
        let dailfile = Dailfile::parse(&content)?;
        tracing::info!("Dailfile parsed successfully");
        let context_dir = std::path::Path::new(dailfile_path.as_str())
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        tracing::info!("Context directory: {}", context_dir.display());
        tracing::info!("Starting BuildExecutor::build...");
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
        if let Some(existing) = lifecycle.get(&jail_name) {
            if existing.status == crate::jail::state::JailStatus::Running {
                anyhow::bail!(
                    "jail '{}' is already running. Use `dail rm --force {}` or `dail stop {}`.",
                    jail_name, jail_name, jail_name
                );
            }
            lifecycle.remove(&jail_name, false)?;
        }

        let state = lifecycle.create(config)?;
        println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);

        if let Some(ip) = info.allocated_ip {
            println!("IP allocated: {} on {}", ip, global.alias_interface);
        }

        println!("Base: {}", info.base_release);
    }

    if let Err(e) = lifecycle.start(&jail_name) {
        tracing::warn!("start failed, attempting to clean up jail '{}'", jail_name);
        // Use force=true to ensure we stop and remove the jail regardless of state
        match lifecycle.remove(&jail_name, true) {
            Ok(()) => {
                tracing::info!(
                    "Successfully cleaned up jail '{}' after failed start",
                    jail_name
                );
            }
            Err(cleanup_err) => {
                tracing::error!(
                    "Failed to clean up jail '{}' after failed start (may need manual cleanup): {}",
                    jail_name,
                    cleanup_err
                );
            }
        }
        return Err(e.into());
    }
    println!("Jail '{}' started.", jail_name);

    if args.rm {
        println!("(will be auto-removed on stop)");
    }

    Ok(())
}
