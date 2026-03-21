use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::network::NetworkConfig;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail inspect myjail
  dail inspect myjail --json   Raw JSON output")]
pub struct InspectArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,
    /// Output raw JSON instead of human-readable format
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: InspectArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new_readonly(global)?;

    let state = lifecycle
        .get(&args.name)
        .ok_or_else(|| anyhow::anyhow!("jail not found: {}", args.name))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(state)?);
        return Ok(());
    }

    let c = &state.config;
    println!("Name:        {}", c.name);
    println!("ID:          {}", state.id);
    println!("Status:      {}", state.status);
    println!("Type:        {:?}", c.jail_type);
    if let Some(jid) = state.jid {
        println!("JID:         {jid}");
    }
    println!("Root:        {}", state.root_path.display());
    if let Some(ref base) = c.base {
        println!("Base:        {base}");
    }
    if let Some(ref hostname) = c.hostname {
        println!("Hostname:    {hostname}");
    }

    match &c.network {
        NetworkConfig::Alias { ip, interface } => {
            println!("Network:     alias ({ip} on {interface})");
        }
        NetworkConfig::Vnet { ip, bridge, gateway } => {
            let gw = gateway.as_deref().unwrap_or("none");
            println!("Network:     vnet ({ip} bridge {bridge} gateway {gw})");
        }
        NetworkConfig::Inherit => println!("Network:     inherit"),
        NetworkConfig::None => println!("Network:     none"),
    }

    if let Some(ref cmd) = c.cmd {
        println!("CMD:         {cmd}");
    }
    if let Some(ref log_file) = c.log_file {
        println!("Log file:    {log_file}");
    }
    if c.persist {
        println!("Persist:     true");
    }
    if c.auto_remove {
        println!("Auto-remove: true");
    }

    if !c.mounts.is_empty() {
        println!("Mounts:");
        for m in &c.mounts {
            let ro = if m.readonly { " (ro)" } else { "" };
            println!("  {} -> {}{ro}", m.source.display(), m.destination.display());
        }
    }
    if !c.params.is_empty() {
        println!("Params:");
        for (k, v) in &c.params {
            println!("  {k} = {v}");
        }
    }
    if !c.limits.is_empty() {
        println!("Limits:");
        for (k, v) in &c.limits {
            println!("  {k} = {v}");
        }
    }

    println!("Created:     {}", state.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    if let Some(t) = state.started_at {
        println!("Started:     {}", t.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(t) = state.stopped_at {
        println!("Stopped:     {}", t.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    Ok(())
}
