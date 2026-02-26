use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail restart myjail")]
pub struct RestartArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: String,
}

pub fn run(args: RestartArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;
    lifecycle.stop(&args.name)?;
    let state = lifecycle.start(&args.name)?;
    println!("Jail '{}' restarted (jid: {})", args.name, state.jid.unwrap_or(0));
    Ok(())
}
