use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::storage;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail snapshot myjail                  Snapshot with tag 'latest'
  dail snapshot myjail --tag v1.0       Snapshot with custom tag")]
pub struct SnapshotArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,
    #[arg(long, default_value = "latest")]
    pub tag: String,
}

pub fn run(args: SnapshotArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new(global.clone())?;

    let state = lifecycle
        .get(&args.name)
        .ok_or_else(|| anyhow::anyhow!("jail not found: {}", args.name))?;

    let backend = storage::get_backend(&global);
    backend.snapshot(&global, state, &args.tag)?;
    println!("Snapshot '{}' created for jail '{}'", args.tag, args.name);

    Ok(())
}
