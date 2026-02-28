use clap::Args;

use crate::jail::config::GlobalConfig;
use crate::storage;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail bootstrap                 Download default base (15.0-RELEASE)
  dail bootstrap 14.2-RELEASE    Download a specific release
  dail bootstrap --list          Show already bootstrapped bases")]
pub struct BootstrapArgs {
    /// FreeBSD release to bootstrap (e.g. 14.2-RELEASE)
    pub release: Option<String>,

    /// List bootstrapped bases
    #[arg(long)]
    pub list: bool,
}

pub fn run(args: BootstrapArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let backend = storage::get_backend(&global);

    if args.list {
        let bases = backend.list_bases(&global)?;
        if bases.is_empty() {
            println!("No bases bootstrapped yet.");
            println!("Use: dail bootstrap <RELEASE> (e.g. dail bootstrap 14.2-RELEASE)");
        } else {
            println!("{:<30} PATH", "RELEASE");
            println!("{}", "-".repeat(60));
            for (name, path) in &bases {
                println!("{:<30} {}", name, path.display());
            }
        }
        return Ok(());
    }

    let release = args.release.unwrap_or(global.default_base.clone());

    println!("Bootstrapping {release}...");
    let path = backend.prepare_base(&global, &release)?;
    println!("Base '{release}' ready at {}", path.display());

    Ok(())
}
