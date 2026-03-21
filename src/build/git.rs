use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

/// Checks whether the string is a remote URL (git repo or direct file link).
pub fn is_remote_url(s: &str) -> bool {
    s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("git://")
        || s.starts_with("git@")
        || s.ends_with(".git")
}

/// Checks whether the string looks like a git repo URL (not a direct file link).
pub fn is_git_url(s: &str) -> bool {
    if s.starts_with("git://") || s.starts_with("git@") || s.ends_with(".git") {
        return true;
    }
    // https/http URLs that end with .dail are direct file downloads, not git repos
    (s.starts_with("https://") || s.starts_with("http://"))
        && !s.ends_with(".dail")
}

/// Result of resolving a git URL: holds the tempdir (for lifetime) and paths.
pub struct GitSource {
    /// Keep alive — dropping this removes the clone.
    pub _tempdir: tempfile::TempDir,
    /// Absolute path to the `.dail` file inside the clone.
    pub dailfile_path: PathBuf,
    /// Absolute path to the context directory (clone root or subdirectory).
    pub context_dir: PathBuf,
    /// Repository name extracted from the URL (e.g. "myapp" from "github.com/user/myapp.git").
    pub repo_name: String,
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

    let repo_name = extract_repo_name(&repo_url);

    Ok(GitSource {
        _tempdir: tempdir,
        dailfile_path,
        context_dir,
        repo_name,
    })
}

/// Extract repository name from a URL.
/// e.g. "https://github.com/user/myapp.git" → "myapp"
fn extract_repo_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("repo")
        .trim_end_matches(".git")
        .to_string()
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

/// Result of fetching a remote .dail file.
pub struct FetchedDailFile {
    pub _tempdir: tempfile::TempDir,
    pub dailfile_path: PathBuf,
}

/// Fetch a single .dail file from an HTTP(S) URL.
pub fn fetch_dail_file(url: &str) -> anyhow::Result<FetchedDailFile> {
    let filename = url.rsplit('/').next().unwrap_or("remote.dail");

    let tempdir = tempfile::tempdir().context("failed to create temp directory")?;
    let dest = tempdir.path().join(filename);

    let output = std::process::Command::new("fetch")
        .args(["-q", "-o"])
        .arg(&dest)
        .arg(url)
        .output()
        .or_else(|_| {
            // fallback to curl if fetch is not available (non-FreeBSD)
            std::process::Command::new("curl")
                .args(["-sSfL", "-o"])
                .arg(&dest)
                .arg(url)
                .output()
        })
        .context("failed to download file — is fetch or curl installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("failed to download {url}: {stderr}");
    }

    Ok(FetchedDailFile {
        _tempdir: tempdir,
        dailfile_path: dest,
    })
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
