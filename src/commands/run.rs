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
  dail run myservice.dail                                   Build .dail file, name from filename
  dail run myservice.dail --name pg                         Build with explicit name
  dail run myservice.dail --rebuild                         Rebuild from scratch
  dail run https://github.com/user/repo.git --name app      Build from git repo
  dail run https://github.com/user/repo//jails/web --name web  Build from subdirectory")]
pub struct RunArgs {
    /// Jail name, .dail file path, or git URL
    #[arg(add = ArgValueCompleter::new(completions::complete_run_first))]
    pub first: Option<String>,

    /// Jail name (overrides name derived from .dail filename)
    #[arg(long)]
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
    #[arg(short = 'm', long = "mount")]
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

    let first = match args.first {
        Some(f) => f,
        None => {
            anyhow::bail!("Missing argument.\nUsage: dail run <name> or dail run <file.dail>");
        }
    };

    fn looks_like_dail_source(s: &str) -> bool {
        s.ends_with(".dail") || git::is_git_url(s)
    }

    // For remote sources, clone/fetch early so we can derive jail name
    let mut _early_git_source = None;
    let jail_name;
    if args.build.is_none() && looks_like_dail_source(&first) {
        jail_name = if let Some(name) = args.name {
            name
        } else if git::is_git_url(&first) {
            let src = git::clone_and_resolve(&first)?;
            let name = src.dailfile_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| src.repo_name.clone());
            _early_git_source = Some(src);
            name
        } else {
            // Local .dail file or HTTP .dail URL — derive from filename
            std::path::Path::new(&first)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("dail-build-{}", &Uuid::new_v4().to_string()[..8]))
        };
        args.build = Some(first);
    } else {
        jail_name = args.name.unwrap_or(first);
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
        // Resolve source: reuse early git clone, clone now, or read local
        let _git_source;
        let (content, context_dir) = if let Some(src) = _early_git_source.take() {
            let content = std::fs::read_to_string(&src.dailfile_path)
                .map_err(|e| anyhow::anyhow!("failed to read .dail file: {e}"))?;
            let ctx = src.context_dir.clone();
            _git_source = Some(src);
            (content, ctx)
        } else if git::is_git_url(dailfile_path) {
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
