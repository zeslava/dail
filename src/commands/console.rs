use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail console myjail                Open shell (/bin/sh)
  dail console myjail --shell /bin/csh")]
pub struct ConsoleArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: String,
    #[arg(long, default_value = "/bin/sh")]
    pub shell: String,
}

pub fn run(args: ConsoleArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new(global)?;

    let status = lifecycle.console(&args.name, &args.shell)?;
    std::process::exit(status.code().unwrap_or(1));
}
