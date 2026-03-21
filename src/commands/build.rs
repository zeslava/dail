use crate::build::dailfile::Dailfile;
use crate::build::executor::BuildExecutor;
use crate::build::git;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use clap::Args;

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail build pg.dail --name myapp             Build from .dail file
  dail build ./jails/web.dail --name web      Build from custom path
  dail build https://github.com/user/repo.git --name app   Build from git repo
  dail build https://github.com/user/repo//jails/web --name web   Build from subdirectory")]
pub struct BuildArgs {
    /// Path to .dail file or git URL
    pub dailfile: String,
    /// Jail name
    #[arg(long)]
    pub name: String,
}

pub fn run(args: BuildArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;

    // Resolve source: git URL or local path
    let _git_source; // hold tempdir alive
    let (content, context_dir) = if git::is_git_url(&args.dailfile) {
        let src = git::clone_and_resolve(&args.dailfile)?;
        let content = std::fs::read_to_string(&src.dailfile_path)
            .map_err(|e| anyhow::anyhow!("failed to read .dail file: {e}"))?;
        let ctx = src.context_dir.clone();
        _git_source = Some(src);
        (content, ctx)
    } else {
        let content = std::fs::read_to_string(&args.dailfile)
            .map_err(|_| anyhow::anyhow!("file not found: {}", args.dailfile))?;
        let ctx = std::path::Path::new(&args.dailfile)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        _git_source = None;
        (content, ctx)
    };

    let dailfile = Dailfile::parse(&content)?;
    BuildExecutor::build(&mut lifecycle, &dailfile, &args.name, None, &context_dir)?;

    println!("Jail '{}' built successfully", args.name);

    Ok(())
}
