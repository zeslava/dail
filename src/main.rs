mod build;
mod commands;
mod completions;
mod error;
mod freebsd;

mod jail;
mod network;
mod output;
mod storage;
mod store;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
use clap_complete::aot::Shell;

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
    /// Open an interactive shell in a jail
    Shell(commands::console::ConsoleArgs),
    /// Build a jail from a .dail file
    Build(commands::build::BuildArgs),
    /// Create a ZFS snapshot of a jail
    Snapshot(commands::snapshot::SnapshotArgs),
    /// Clone a jail from a snapshot
    Clone(commands::clone::CloneArgs),
    /// Create and start a jail in one step
    Run(commands::run::RunArgs),
    /// Show resource usage of running jails
    Top(commands::top::TopArgs),
    /// List available presets
    Preset(commands::preset::PresetArgs),
    /// Manage build cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
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
    // Initialize tracing with INFO level by default (no RUST_LOG needed)
    // Respects RUST_LOG env var if set for finer control
    let env_filter = if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::EnvFilter::from_default_env()
    } else {
        tracing_subscriber::EnvFilter::new("dail=info")
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .with_ansi(true)
        .init();

    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    // Most commands require root privileges (jail(2), mount, etc.)
    // Read-only commands don't need root
    let requires_root = !matches!(
        cli.command,
        Commands::Completions { .. }
            | Commands::Ls(..)
            | Commands::Inspect(..)
            | Commands::Logs(..)
            | Commands::Config {
                action: ConfigAction::Show
            }
            | Commands::Top(..)
            | Commands::Preset(..)
    );

    if requires_root {
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
        Commands::Shell(args) => commands::console::run(args)?,
        Commands::Build(args) => commands::build::run(args)?,
        Commands::Snapshot(args) => commands::snapshot::run(args)?,
        Commands::Clone(args) => commands::clone::run(args)?,
        Commands::Run(args) => commands::run::run(args)?,
        Commands::Top(args) => commands::top::run(args)?,
        Commands::Preset(args) => commands::preset::run(args)?,
        Commands::Cache { action } => match action {
            CacheAction::Clean => commands::cache::clean()?,
        },
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
            println!(
                "  dail completions zsh | doas tee /usr/local/share/zsh/site-functions/_dail > /dev/null"
            );
            println!(
                "  dail completions bash | doas tee /usr/local/etc/bash_completion.d/dail > /dev/null"
            );
            println!("  dail completions fish > ~/.config/fish/completions/dail.fish");
        }
    }

    Ok(())
}
