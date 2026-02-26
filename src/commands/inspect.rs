use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail inspect myjail")]
pub struct InspectArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,
}

pub fn run(args: InspectArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new(global)?;

    let state = lifecycle
        .get(&args.name)
        .ok_or_else(|| anyhow::anyhow!("jail not found: {}", args.name))?;

    let json = serde_json::to_string_pretty(state)?;
    println!("{json}");

    Ok(())
}
