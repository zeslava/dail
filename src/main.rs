mod commands;
mod error;
mod freebsd;
mod jail;
mod network;
mod output;
mod storage;
mod store;
mod build;
mod image;
mod completions;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::aot::Shell;
use clap_complete::CompleteEnv;

#[derive(Parser)]
#[command(name = "dail", about = "FreeBSD jail manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage dail configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Download and extract a FreeBSD base system
    Bootstrap(commands::bootstrap::BootstrapArgs),
    /// Create a new jail
    Create(commands::create::CreateArgs),
    /// Start a jail
    Start(commands::start::StartArgs),
    /// Stop a jail
    Stop(commands::stop::StopArgs),
    /// Restart a jail
    Restart(commands::restart::RestartArgs),
    /// Remove a jail
    Rm(commands::rm::RmArgs),
    /// View jail logs
    Logs(commands::logs::LogsArgs),
    /// List jails
    Ls(commands::ls::LsArgs),
    /// Show detailed jail information
    Inspect(commands::inspect::InspectArgs),
    /// Execute a command in a running jail
    Exec(commands::exec::ExecArgs),
    /// Open an interactive console in a jail
    Console(commands::console::ConsoleArgs),
    /// Build a jail from a Dailfile
    Build(commands::build::BuildArgs),
    /// Create a ZFS snapshot of a jail
    Snapshot(commands::snapshot::SnapshotArgs),
    /// Clone a jail from a snapshot
    Clone(commands::clone::CloneArgs),
    /// Create and start a jail in one step
    Run(commands::run::RunArgs),
    /// List available presets
    Preset(commands::preset::PresetArgs),
    /// Manage build cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Manage images
    Image {
        #[command(subcommand)]
        action: commands::image::ImageAction,
    },
    /// List local images (alias for `image ls`)
    Images,
    /// Generate shell completions
    #[command(after_long_help = "\
Dynamic completions (add to shell rc, enables live jail name completion):
  zsh:  echo 'source <(COMPLETE=zsh dail)' >> ~/.zshrc
  bash: echo 'source <(COMPLETE=bash dail)' >> ~/.bashrc
  fish: COMPLETE=fish dail > ~/.config/fish/completions/dail.fish

Static completions (subcommands only, no live jail names):
  dail completions zsh | doas tee /usr/local/share/zsh/site-functions/_dail > /dev/null
  dail completions bash | doas tee /usr/local/etc/bash_completion.d/dail > /dev/null
  dail completions fish > ~/.config/fish/completions/dail.fish")]
    Completions {
        /// Shell to generate completions for (bash, zsh, fish)
        shell: Option<Shell>,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Remove cached packages
    Clean,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Initialize dail on this host
    Init {
        /// ZFS pool to use for storage
        #[arg(long)]
        zfs_pool: Option<String>,
    },
    /// Display current configuration
    Show,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dail=info".parse().unwrap()),
        )
        .init();

    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Most commands require root privileges (jail(2), mount, etc.)
    if !matches!(cli.command, Commands::Completions { .. }) {
        // SAFETY: getuid is always safe to call, no side effects
        if unsafe { libc::getuid() } != 0 {
            anyhow::bail!("dail requires root privileges. Run with doas or sudo.");
        }
    }

    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::Init { zfs_pool } => commands::config_init::run(zfs_pool)?,
            ConfigAction::Show => commands::config_show::run()?,
        },
        Commands::Bootstrap(args) => commands::bootstrap::run(args)?,
        Commands::Create(args) => commands::create::run(args)?,
        Commands::Start(args) => commands::start::run(args)?,
        Commands::Stop(args) => commands::stop::run(args)?,
        Commands::Restart(args) => commands::restart::run(args)?,
        Commands::Rm(args) => commands::rm::run(args)?,
        Commands::Logs(args) => commands::logs::run(args)?,
        Commands::Ls(args) => commands::ls::run(args)?,
        Commands::Inspect(args) => commands::inspect::run(args)?,
        Commands::Exec(args) => commands::exec::run(args)?,
        Commands::Console(args) => commands::console::run(args)?,
        Commands::Build(args) => commands::build::run(args)?,
        Commands::Snapshot(args) => commands::snapshot::run(args)?,
        Commands::Clone(args) => commands::clone::run(args)?,
        Commands::Run(args) => commands::run::run(args)?,
        Commands::Preset(args) => commands::preset::run(args)?,
        Commands::Cache { action } => match action {
            CacheAction::Clean => commands::cache::clean()?,
        },
        Commands::Image { action } => commands::image::run(action)?,
        Commands::Images => commands::image::run(commands::image::ImageAction::Ls { quiet: false })?,
        Commands::Completions { shell: Some(shell) } => {
            clap_complete::generate(shell, &mut Cli::command(), "dail", &mut std::io::stdout());
        }
        Commands::Completions { shell: None } => {
            println!("Generate shell completions for dail.\n");
            println!("Dynamic completions (add to shell rc, enables live jail name completion):");
            println!("  zsh:  echo 'source <(COMPLETE=zsh dail)' >> ~/.zshrc");
            println!("  bash: echo 'source <(COMPLETE=bash dail)' >> ~/.bashrc");
            println!("  fish: COMPLETE=fish dail > ~/.config/fish/completions/dail.fish");
            println!();
            println!("Static completions (subcommands only, no live jail names):");
            println!("  dail completions zsh | doas tee /usr/local/share/zsh/site-functions/_dail > /dev/null");
            println!("  dail completions bash | doas tee /usr/local/etc/bash_completion.d/dail > /dev/null");
            println!("  dail completions fish > ~/.config/fish/completions/dail.fish");
        }
    }

    Ok(())
}
