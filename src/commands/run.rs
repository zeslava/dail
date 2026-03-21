use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use uuid::Uuid;

use crate::build::dailfile::Dailfile;
use crate::build::executor::BuildExecutor;
use crate::build::git;
use crate::commands::shared::{self, CommonJailArgs, ConfigFlags};
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail run pg                                               Create and start with defaults
  dail run pg --preset postgres                             Apply postgres preset
  dail run pg --rm                                          Auto-remove on stop
  dail run ./examples/postgres/pg.dail pg                   Build .dail file, name jail 'pg'
  dail run ./examples/postgres/pg.dail                      Build with auto-generated name
  dail run ./examples/postgres/pg.dail pg --rebuild         Rebuild from scratch
  dail run https://github.com/user/repo.git pg              Build from git repo
  dail run https://github.com/user/repo//jails/web pg       Build from subdirectory")]
pub struct RunArgs {
    /// Jail name, .dail file path, or git URL
    #[arg(add = ArgValueCompleter::new(completions::complete_run_first))]
    pub first: Option<String>,

    /// Jail name (when first argument is a .dail file path)
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

    /// Build from .dail file before starting
    #[arg(long)]
    pub build: Option<String>,

    /// Remove existing jail and rebuild
    #[arg(long)]
    pub rebuild: bool,

    /// Publish port: [host_ip:]host_port:jail_port[/proto]
    #[arg(short = 'p', long = "publish")]
    pub publish: Vec<String>,
}


pub fn run(mut args: RunArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    // Resolve first/second positional args into (jail_name, optional dailfile_path)
    let first = match args.first {
        Some(f) => f,
        None => {
            anyhow::bail!(
                "Missing argument.\nUsage: dail run <name> or dail run <file.dail> [name]"
            );
        }
    };

    fn looks_like_dail_file(s: &str) -> bool {
        s.ends_with(".dail") || s.contains('/') || git::is_git_url(s)
    }

    let jail_name;
    if args.build.is_none() && looks_like_dail_file(&first) {
        // first arg is a .dail file path, second (if any) is the jail name
        jail_name = args.second.unwrap_or_else(|| {
            // Derive name from filename: "myservice.dail" → "myservice"
            std::path::Path::new(&first)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("dail-build-{}", &Uuid::new_v4().to_string()[..8]))
        });
        args.build = Some(first);
    } else {
        // first arg is the jail name
        if args.second.is_some() {
            anyhow::bail!("unexpected second argument. Did you mean: dail run <file.dail> <name>?");
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
        publish: &args.publish,
    };

    let flags = ConfigFlags {
        auto_remove: args.rm,
        ..Default::default()
    };

    let (config, info) = shared::build_jail_config(jail_name.clone(), &common, &global, flags)?;

    if let Some(ref dailfile_path) = args.build {
        // --build mode: ignore create args, use .dail file
        if let Some(existing) = lifecycle.get(&jail_name) {
            if args.rebuild {
                lifecycle.remove(&jail_name, true)?;
                println!("Jail '{}' removed for rebuild.", jail_name);
            } else if existing.is_stopped() || existing.status == crate::jail::state::JailStatus::Idle {
                lifecycle.remove(&jail_name, existing.status == crate::jail::state::JailStatus::Idle)?;
                println!("Jail '{}' removed for rebuild.", jail_name);
            } else {
                anyhow::bail!(
                    "Jail '{}' is running. Use --rebuild to force rebuild.",
                    jail_name
                );
            }
        }
        // Resolve source: git URL or local path
        let _git_source;
        let (content, context_dir) = if git::is_git_url(dailfile_path) {
            tracing::info!("Cloning git repository: {}", dailfile_path);
            let src = git::clone_and_resolve(dailfile_path)?;
            let content = std::fs::read_to_string(&src.dailfile_path)
                .map_err(|e| anyhow::anyhow!("failed to read .dail file: {e}"))?;
            let ctx = src.context_dir.clone();
            _git_source = Some(src);
            (content, ctx)
        } else {
            tracing::info!("Reading {}", dailfile_path);
            let content = std::fs::read_to_string(dailfile_path)
                .map_err(|_| anyhow::anyhow!("file not found: {dailfile_path}"))?;
            let ctx = std::path::Path::new(dailfile_path.as_str())
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            _git_source = None;
            (content, ctx)
        };

        tracing::info!("File read successfully ({} bytes)", content.len());
        let dailfile = Dailfile::parse(&content)?;
        tracing::info!("Parsed successfully");
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
            match existing.status {
                crate::jail::state::JailStatus::Running => {
                    anyhow::bail!(
                        "jail '{}' is already running. Use `dail rm --force {}` or `dail stop {}`.",
                        jail_name, jail_name, jail_name
                    );
                }
                crate::jail::state::JailStatus::Idle => {
                    // Jail in kernel but no processes — stop and restart
                    lifecycle.stop(&jail_name)?;
                    println!("Jail '{}' stopped (was idle).", jail_name);
                }
                crate::jail::state::JailStatus::Stopped | crate::jail::state::JailStatus::Created => {
                    // Existing jail, just start it
                }
            }
        } else {
            let state = lifecycle.create(config)?;
            println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);

            if let Some(ip) = info.allocated_ip {
                println!("IP allocated: {} on {}", ip, global.alias_interface);
            }

            println!("Base: {}", info.base_release);
        }
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
