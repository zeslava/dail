use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum JailError {
    #[error("jail command failed: {0}")]
    CommandFailed(String),
    #[error("jail not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

/// Parameters passed to `jail -c` or `jail_set(2)`.
#[derive(Debug, Clone)]
pub struct JailParams {
    pub name: String,
    pub path: PathBuf,
    pub hostname: Option<String>,
    pub ip4_addr: Option<String>,
    pub vnet: bool,
    pub persist: bool,
    pub extra: HashMap<String, String>,
}

/// A running jail as reported by `jls`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailInfo {
    #[serde(deserialize_with = "deserialize_jid")]
    pub jid: i32,
    pub name: String,
    pub path: String,
    #[serde(default, rename = "host.hostname")]
    pub hostname: String,
    #[serde(default)]
    pub ip4: String,
    #[serde(default)]
    pub meta: String,
}

fn deserialize_jid<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    s.parse().map_err(serde::de::Error::custom)
}

/// Low-level jail operations. Shells out to `jail(8)` / `jls(8)` / `jexec(8)`.
pub struct JailSys;

impl JailSys {
    /// Create and start a jail.
    pub fn create(params: &JailParams) -> Result<i32, JailError> {
        let mut args = vec![
            "-i".to_string(),
            "-c".to_string(),
            format!("name={}", params.name),
            format!("path={}", params.path.display()),
            format!("host.hostname={}", params.hostname.as_deref().unwrap_or(&params.name)),
        ];

        if let Some(ref ip) = params.ip4_addr {
            args.push(format!("ip4.addr={ip}"));
        }
        if params.vnet {
            args.push("vnet=new".to_string());
        }
        if params.persist {
            args.push("persist".to_string());
        }
        args.push("meta=dail".to_string());
        for (k, v) in &params.extra {
            args.push(format!("{k}={v}"));
        }

        let output = std::process::Command::new("jail")
            .args(&args)
            .output()?;

        if !output.status.success() {
            return Err(JailError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Parse JID from output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let jid = stdout
            .trim()
            .parse::<i32>()
            .map_err(|e| JailError::Parse(format!("failed to parse JID: {e}, output: {stdout}")))?;

        Ok(jid)
    }

    /// Stop and remove a jail.
    pub fn remove(name: &str) -> Result<(), JailError> {
        let output = std::process::Command::new("jail")
            .args(["-r", name])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Err(JailError::NotFound(name.to_string()));
            }
            return Err(JailError::CommandFailed(stderr.to_string()));
        }
        Ok(())
    }

    /// List running jails via `jls --libxo json`.
    pub fn list() -> Result<Vec<JailInfo>, JailError> {
        let output = std::process::Command::new("jls")
            .args(["--libxo", "json", "-n", "-q"])
            .output()?;

        if !output.status.success() {
            return Err(JailError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct JlsOutput {
            #[serde(rename = "jail-information")]
            jail_information: JlsJailInfo,
        }
        #[derive(Deserialize)]
        struct JlsJailInfo {
            jail: Vec<JailInfo>,
        }

        let parsed: JlsOutput =
            serde_json::from_str(&stdout).map_err(|e| JailError::Parse(e.to_string()))?;

        Ok(parsed.jail_information.jail)
    }

    /// Execute a command inside a jail.
    pub fn exec(name: &str, command: &[&str]) -> Result<std::process::ExitStatus, JailError> {
        let status = std::process::Command::new("jexec")
            .arg(name)
            .args(command)
            .status()?;
        Ok(status)
    }

    /// Execute a command inside a jail with stdout/stderr redirected to a log file.
    /// The process is spawned detached (does not block).
    pub fn exec_logged(
        name: &str,
        cmd: &str,
        log_path: &std::path::Path,
        env: &std::collections::HashMap<String, String>,
    ) -> Result<(), JailError> {
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let stderr_file = log_file.try_clone()?;

        let mut args = vec!["env".to_string()];
        for (k, v) in env {
            args.push(format!("{k}={v}"));
        }
        args.extend(["/bin/sh".to_string(), "-c".to_string(), cmd.to_string()]);

        std::process::Command::new("jexec")
            .arg(name)
            .args(&args)
            .stdout(log_file)
            .stderr(stderr_file)
            .stdin(std::process::Stdio::null())
            .spawn()?;

        Ok(())
    }

    /// Attach an interactive console to a jail.
    pub fn console(name: &str, shell: &str) -> Result<std::process::ExitStatus, JailError> {
        let status = std::process::Command::new("jexec")
            .arg(name)
            .arg(shell)
            .status()?;
        Ok(status)
    }
}
