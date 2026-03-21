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
  dail build https://example.com/myapp.dail --name app     Build from remote .dail file")]
pub struct BuildArgs {
    /// Path to .dail file, git URL, or HTTP URL
    pub dailfile: String,
    /// Jail name
    #[arg(long)]
    pub name: String,
}

pub fn run(args: BuildArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let mut lifecycle = JailLifecycle::new(global)?;

    // Resolve source: git repo, HTTP .dail file, or local path
    let _remote_source: Option<Box<dyn std::any::Any>>;
    let (content, context_dir) = if git::is_git_url(&args.dailfile) {
        let src = git::clone_and_resolve(&args.dailfile)?;
        let content = std::fs::read_to_string(&src.dailfile_path)
            .map_err(|e| anyhow::anyhow!("failed to read .dail file: {e}"))?;
        let ctx = src.context_dir.clone();
        _remote_source = Some(Box::new(src));
        (content, ctx)
    } else if git::is_remote_url(&args.dailfile) {
        let fetched = git::fetch_dail_file(&args.dailfile)?;
        let content = std::fs::read_to_string(&fetched.dailfile_path)
            .map_err(|e| anyhow::anyhow!("failed to read .dail file: {e}"))?;
        let ctx = fetched.dailfile_path.parent().unwrap().to_path_buf();
        _remote_source = Some(Box::new(fetched));
        (content, ctx)
    } else {
        let content = std::fs::read_to_string(&args.dailfile)
            .map_err(|_| anyhow::anyhow!("file not found: {}", args.dailfile))?;
        let ctx = std::path::Path::new(&args.dailfile)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        _remote_source = None;
        (content, ctx)
    };

    let dailfile = Dailfile::parse(&content)?;
    BuildExecutor::build(&mut lifecycle, &dailfile, &args.name, None, &context_dir)?;

    println!("Jail '{}' built successfully", args.name);

    Ok(())
}
