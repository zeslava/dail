use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail stop myjail")]
pub struct StopArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: String,
}

pub fn run(args: StopArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;
    lifecycle.stop(&args.name)?;
    println!("Jail '{}' stopped", args.name);
    Ok(())
}
