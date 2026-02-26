#[derive(Debug, thiserror::Error)]
pub enum RctlError {
    #[error("rctl command failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Rctl;

impl Rctl {
    /// Add a resource limit for a jail.
    /// Example: `add_limit("myjail", "maxproc", "100")`
    pub fn add_limit(jail_name: &str, resource: &str, value: &str) -> Result<(), RctlError> {
        let rule = format!("jail:{jail_name}:{resource}:deny={value}");
        let output = std::process::Command::new("rctl")
            .args(["-a", &rule])
            .output()?;

        if !output.status.success() {
            return Err(RctlError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Remove all limits for a jail.
    pub fn remove_limits(jail_name: &str) -> Result<(), RctlError> {
        let filter = format!("jail:{jail_name}");
        let output = std::process::Command::new("rctl")
            .args(["-r", &filter])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Not an error if no rules exist
            if !stderr.contains("No matching rule") {
                return Err(RctlError::CommandFailed(stderr.to_string()));
            }
        }
        Ok(())
    }

    /// List limits for a jail.
    pub fn list_limits(jail_name: &str) -> Result<Vec<String>, RctlError> {
        let filter = format!("jail:{jail_name}");
        let output = std::process::Command::new("rctl")
            .args(["-l", &filter])
            .output()?;

        if !output.status.success() {
            return Err(RctlError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }
}
