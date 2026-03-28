use crate::commands::shared::{self, CommonJailArgs, ConfigFlags};
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use clap::Args;
use clap_complete::engine::ArgValueCompleter;

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
    #[arg(short = 'm', long = "mount")]
    pub mounts: Vec<String>,

    /// Read-only nullfs mount (host:jail)
    #[arg(long = "mount-ro")]
    pub mounts_ro: Vec<String>,

    /// Environment variable (KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub envs: Vec<String>,

    /// Hostname
    #[arg(long)]
    pub hostname: Option<String>,

    /// Allow parameters (e.g. raw_sockets)
    #[arg(long = "allow")]
    pub allows: Vec<String>,

    /// Resource limits (e.g. maxproc=100)
    #[arg(long = "limit")]
    pub limits: Vec<String>,

    /// Destroy jail when no processes are running
    #[arg(long)]
    pub no_persist: bool,

    /// Apply a preset (e.g. postgres, nginx, dev)
    #[arg(long, add = ArgValueCompleter::new(completions::complete_preset_names))]
    pub preset: Option<String>,

    /// Network mode: 'inherit' (host network), 'none' (isolated), or omit for auto IP from pool
    #[arg(long = "network", add = ArgValueCompleter::new(completions::complete_network_mode))]
    pub network: Option<String>,

    /// Publish port: [host_ip:]host_port:jail_port[/proto]
    #[arg(short = 'p', long = "publish")]
    pub publish: Vec<String>,

    /// UID for the service user created by SERVICE directive
    #[arg(long)]
    pub uid: Option<u32>,
}

pub fn run(args: CreateArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

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
        no_persist: args.no_persist,
        preset: args.preset.as_deref(),
        network: args.network.as_deref(),
        publish: &args.publish,
        envs: &args.envs,
        uid: args.uid,
    };

    let (config, info) =
        shared::build_jail_config(args.name.clone(), &common, &global, ConfigFlags::default())?;

    let state = lifecycle.create(config)?;
    println!("Jail '{}' created (id: {})", state.name(), &state.id[..8]);

    if let Some(ip) = info.allocated_ip {
        println!("IP allocated: {} on {}", ip, global.alias_interface);
    }

    println!("Base: {}", info.base_release);
    println!("Root: {}", state.root_path.display());

    Ok(())
}
