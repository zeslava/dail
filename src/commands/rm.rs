use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail rm myjail               Remove a stopped jail
  dail rm myjail --force       Stop and remove a running jail")]
pub struct RmArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: RmArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;
    lifecycle.remove(&args.name, args.force)?;
    println!("Jail '{}' removed", args.name);
    Ok(())
}
