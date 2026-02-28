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

    let mut table = crate::output::Table::new(vec!["NAME", "SOURCE", "DESCRIPTION"]);
    for p in &presets {
        table.add_row(vec![p.name.clone(), p.source.to_string(), p.description.clone()]);
    }
    table.print();

    Ok(())
}
