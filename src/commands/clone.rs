use clap::Args;
use clap_complete::engine::ArgValueCompleter;
use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::storage;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail clone myjail myjail-copy          Clone from latest snapshot
  dail clone myjail:v1.0 myjail-copy     Clone from tagged snapshot")]
pub struct CloneArgs {
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub source: String,
    pub new_name: String,
}

pub fn run(args: CloneArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global.clone())?;

    let (source_name, tag) = if let Some((name, tag)) = args.source.split_once(':') {
        (name, tag)
    } else {
        (args.source.as_str(), "latest")
    };

    let source = lifecycle
        .get(source_name)
        .ok_or_else(|| anyhow::anyhow!("jail not found: {source_name}"))?
        .clone();

    let backend = storage::get_backend(&global);
    let _new_root = backend.clone_jail(&global, &source, tag, &args.new_name)?;

    let mut new_config = source.config.clone();
    new_config.name = args.new_name.clone();
    lifecycle.create(new_config)?;

    println!("Jail '{}' cloned to '{}'", source_name, args.new_name);

    Ok(())
}
