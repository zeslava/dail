use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail rm myjail               Remove a stopped jail
  dail rm myjail --force       Stop and remove a running jail
  dail rm --all --force        Remove all jails")]
pub struct RmArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: Option<String>,
    #[arg(long)]
    pub force: bool,
    /// Remove all jails
    #[arg(long)]
    pub all: bool,
}

pub fn run(args: RmArgs) -> anyhow::Result<()> {
    if !args.all && args.name.is_none() {
        anyhow::bail!("provide a jail name or use --all");
    }

    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;

    let names: Vec<String> = if args.all {
        lifecycle.list().iter().map(|j| j.name().to_string()).collect()
    } else {
        vec![args.name.unwrap()]
    };

    for name in &names {
        lifecycle.remove(name, args.force)?;
        println!("Jail '{name}' removed");
    }

    Ok(())
}
