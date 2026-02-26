use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail start myjail")]
pub struct StartArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,
}

pub fn run(args: StartArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;

    let state = lifecycle.start(&args.name)?;
    println!("Jail '{}' started (jid: {})", args.name, state.jid.unwrap_or(0));

    Ok(())
}
