use clap::Args;

use crate::jail::config::GlobalConfig;
use crate::jail::preset::Preset;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail preset                   List all available presets")]
pub struct PresetArgs;

pub fn run(_args: PresetArgs) -> anyhow::Result<()> {
    let config = GlobalConfig::load()?;
    let presets = Preset::list_all(&config);

    if presets.is_empty() {
        println!("No presets available.");
        return Ok(());
    }

    println!("{:<15} {:<10} {}", "NAME", "SOURCE", "DESCRIPTION");
    for p in &presets {
        println!("{:<15} {:<10} {}", p.name, p.source, p.description);
    }

    Ok(())
}
