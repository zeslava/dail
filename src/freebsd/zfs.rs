#[derive(Debug, thiserror::Error)]
pub enum ZfsError {
    #[error("zfs command failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Zfs;

impl Zfs {
    /// Create a ZFS dataset.
    pub fn create_dataset(name: &str) -> Result<(), ZfsError> {
        run_zfs(&["create", "-p", name])
    }

    /// Destroy a ZFS dataset.
    pub fn destroy_dataset(name: &str, recursive: bool) -> Result<(), ZfsError> {
        if recursive {
            run_zfs(&["destroy", "-rf", name])
        } else {
            run_zfs(&["destroy", name])
        }
    }

    /// Create a snapshot.
    pub fn snapshot(dataset: &str, snap_name: &str) -> Result<(), ZfsError> {
        let snap = format!("{dataset}@{snap_name}");
        run_zfs(&["snapshot", &snap])
    }

    /// Clone a snapshot to a new dataset.
    pub fn clone(snapshot: &str, target: &str) -> Result<(), ZfsError> {
        run_zfs(&["clone", snapshot, target])
    }

    /// Get a ZFS property value for a dataset.
    pub fn get_property(dataset: &str, property: &str) -> Result<String, ZfsError> {
        let output = std::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", property, dataset])
            .output()?;

        if !output.status.success() {
            return Err(ZfsError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the mountpoint of a dataset.
    pub fn get_mountpoint(dataset: &str) -> Result<String, ZfsError> {
        Self::get_property(dataset, "mountpoint")
    }

    /// Check if a dataset exists.
    pub fn dataset_exists(name: &str) -> Result<bool, ZfsError> {
        let output = std::process::Command::new("zfs")
            .args(["list", "-H", name])
            .output()?;

        Ok(output.status.success())
    }

}

fn run_zfs(args: &[&str]) -> Result<(), ZfsError> {
    let output = std::process::Command::new("zfs")
        .args(args)
        .output()?;

    if !output.status.success() {
        return Err(ZfsError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}
