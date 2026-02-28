use std::io::{Read, Seek, SeekFrom, BufRead, BufReader};
use std::path::PathBuf;

use clap::Args;
use clap_complete::engine::ArgValueCompleter;

use crate::completions;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;

#[derive(Args)]
#[command(after_help = "\
Examples:
  dail logs myjail
  dail logs myjail --tail 20
  dail logs myjail -f
  dail logs myjail --file /var/log/messages")]
pub struct LogsArgs {
    /// Jail name
    #[arg(add = ArgValueCompleter::new(completions::complete_jail_names))]
    pub name: String,

    /// Read a file from jail rootfs instead of cmd.log
    #[arg(long)]
    pub file: Option<String>,

    /// Follow log output (like tail -f)
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show from the end
    #[arg(long)]
    pub tail: Option<usize>,
}

pub fn run(args: LogsArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new_readonly(global.clone())?;

    let state = lifecycle
        .get(&args.name)
        .ok_or_else(|| anyhow::anyhow!("jail not found: {}", args.name))?;

    let path = if let Some(ref file) = args.file {
        let rel = file.strip_prefix('/').unwrap_or(file);
        let resolved = state.root_path.join(rel);
        // Prevent path traversal outside jail root
        let canonical = resolved.canonicalize()
            .map_err(|_| anyhow::anyhow!("log file not found: {}", resolved.display()))?;
        let root_canonical = state.root_path.canonicalize()
            .map_err(|e| anyhow::anyhow!("cannot resolve jail root: {e}"))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("path escapes jail root: {}", file);
        }
        canonical
    } else if let Some(ref log_file) = state.config.log_file {
        let rel = log_file.strip_prefix('/').unwrap_or(log_file);
        state.root_path.join(rel)
    } else {
        global.jails_dir().join(&args.name).join("cmd.log")
    };

    if !path.exists() {
        anyhow::bail!("log file not found: {}", path.display());
    }

    if let Some(n) = args.tail {
        print_tail(&path, n)?;
    } else if !args.follow {
        let content = std::fs::read_to_string(&path)?;
        print!("{content}");
    }

    if args.follow {
        follow(&path, args.tail.is_none())?;
    }

    Ok(())
}

fn print_tail(path: &PathBuf, n: usize) -> anyhow::Result<()> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}

fn follow(path: &PathBuf, print_existing: bool) -> anyhow::Result<()> {
    let mut file = std::fs::File::open(path)?;

    if print_existing {
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        print!("{buf}");
    } else {
        file.seek(SeekFrom::End(0))?;
    }

    loop {
        let mut buf = String::new();
        let n = file.read_to_string(&mut buf)?;
        if n > 0 {
            print!("{buf}");
        } else {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
}
