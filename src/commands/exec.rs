use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail exec myjail ls /etc     Run a command in a jail
  dail exec myjail pkg install -y nginx")]
pub struct ExecArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: String,
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,
}

pub fn run(args: ExecArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new(global)?;

    let cmd_refs: Vec<&str> = args.command.iter().map(|s| s.as_str()).collect();
    let status = lifecycle.exec(&args.name, &cmd_refs)?;

    std::process::exit(status.code().unwrap_or(1));
}
