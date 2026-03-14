use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::jail::state::JailStatus;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail stop myjail
  dail stop --all              Stop all running jails")]
pub struct StopArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: Option<String>,
    /// Stop all running jails
    #[arg(long)]
    pub all: bool,
}

pub fn run(args: StopArgs) -> anyhow::Result<()> {
    if !args.all && args.name.is_none() {
        anyhow::bail!("provide a jail name or use --all");
    }

    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;

    let names: Vec<String> = if args.all {
        lifecycle.list().iter()
            .filter(|j| j.status == JailStatus::Running || j.status == JailStatus::Idle)
            .map(|j| j.name().to_string())
            .collect()
    } else {
        vec![args.name.unwrap()]
    };

    for name in &names {
        lifecycle.stop(name)?;
        println!("Jail '{name}' stopped");
    }

    Ok(())
}
