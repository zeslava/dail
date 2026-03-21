use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

/// Checks whether the string looks like a git URL.
///
/// Recognized forms:
/// - `https://...` / `http://...`
/// - `git://...`
/// - `git@host:path`
/// - any URL ending with `.git`
pub fn is_git_url(s: &str) -> bool {
    s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("git://")
        || s.starts_with("git@")
        || s.ends_with(".git")
}

/// Result of resolving a git URL: holds the tempdir (for lifetime) and paths.
pub struct GitSource {
    /// Keep alive — dropping this removes the clone.
    pub _tempdir: tempfile::TempDir,
    /// Absolute path to the `.dail` file inside the clone.
    pub dailfile_path: PathBuf,
    /// Absolute path to the context directory (clone root or subdirectory).
    pub context_dir: PathBuf,
}

/// Clone a git URL and locate the `.dail` file inside.
///
/// The URL may contain a path fragment after the repo to point at a subdirectory:
///   `https://github.com/user/repo.git//path/to/dir`
///   `https://github.com/user/repo//subdir`
///
/// The double-slash (`//`) separates the repo URL from the subdirectory inside.
/// If no `//` is present, the repo root is used.
///
/// Inside the resolved directory the function looks for:
/// 1. A single `.dail` file — uses it.
/// 2. Multiple `.dail` files — error, ambiguous.
/// 3. No `.dail` files — error.
pub fn clone_and_resolve(url: &str) -> anyhow::Result<GitSource> {
    let (repo_url, subdir) = split_subdir(url);

    let tempdir = tempfile::tempdir().context("failed to create temp directory for git clone")?;

    let output = std::process::Command::new("git")
        .args(["clone", "--depth=1", "--single-branch", &repo_url])
        .arg(tempdir.path().join("repo"))
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("failed to run git — is git installed?")?;

    if !output.status.success() {
        bail!("git clone failed for: {repo_url}");
    }

    let clone_root = tempdir.path().join("repo");
    let search_dir = match &subdir {
        Some(sub) => {
            let dir = clone_root.join(sub);
            if !dir.is_dir() {
                bail!("subdirectory '{}' not found in cloned repo", sub);
            }
            dir
        }
        None => clone_root.clone(),
    };

    let dail_files = find_dail_files(&search_dir)?;

    let dailfile_path = match dail_files.len() {
        0 => bail!(
            "no .dail file found in {}",
            subdir.as_deref().unwrap_or("repository root")
        ),
        1 => dail_files.into_iter().next().unwrap(),
        n => bail!(
            "found {} .dail files in {} — specify the subdirectory with // in the URL (e.g. url//path/to/dir)",
            n,
            subdir.as_deref().unwrap_or("repository root")
        ),
    };

    let context_dir = dailfile_path
        .parent()
        .unwrap_or(&search_dir)
        .to_path_buf();

    Ok(GitSource {
        _tempdir: tempdir,
        dailfile_path,
        context_dir,
    })
}

/// Split `url//subdir` into `(url, Some(subdir))`.
/// If no `//` separator, returns `(url, None)`.
fn split_subdir(url: &str) -> (String, Option<String>) {
    // Skip the protocol part (e.g. https://) before looking for //
    let proto_end = if let Some(pos) = url.find("://") {
        pos + 3
    } else if url.starts_with("git@") {
        // git@host:user/repo//subdir — find first / after ':'
        url.find(':').map(|p| p + 1).unwrap_or(0)
    } else {
        0
    };

    if let Some(pos) = url[proto_end..].find("//") {
        let abs_pos = proto_end + pos;
        let repo = url[..abs_pos].to_string();
        let sub = url[abs_pos + 2..].trim_matches('/').to_string();
        if sub.is_empty() {
            (repo, None)
        } else {
            (repo, Some(sub))
        }
    } else {
        (url.to_string(), None)
    }
}

/// Find `.dail` files in the top level of `dir` (non-recursive).
fn find_dail_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(dir).context("failed to read directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "dail" {
                    result.push(path);
                }
            }
        }
    }
    Ok(result)
}
