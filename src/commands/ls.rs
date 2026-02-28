use clap::Args;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::jail::state::JailStatus;
use crate::output;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail ls                      List all jails
  dail ls --running            Only running jails
  dail ls --format json        Output as JSON")]
pub struct LsArgs {
    /// Show only running jails
    #[arg(long)]
    pub running: bool,

    /// Output format (table or json)
    #[arg(long, default_value = "table")]
    pub format: String,

    /// Output names only (for scripting)
    #[arg(short, long)]
    pub quiet: bool,
}

pub fn run(args: LsArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new_readonly(global)?;

    let jails: Vec<_> = lifecycle
        .list()
        .into_iter()
        .filter(|j| !args.running || j.status == JailStatus::Running)
        .collect();

    if args.quiet {
        output::print_names(&jails);
    } else {
        match args.format.as_str() {
            "json" => output::print_json(&jails)?,
            _ => output::print_table(&jails),
        }
    }

    Ok(())
}
