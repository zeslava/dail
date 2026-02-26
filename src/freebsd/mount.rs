use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MountError {
    #[error("mount failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct NullfsMount;

impl NullfsMount {
    /// Mount a nullfs filesystem.
    pub fn mount(source: &Path, target: &Path, readonly: bool) -> Result<(), MountError> {
        let mut args = vec!["-t", "nullfs"];
        if readonly {
            args.extend(["-o", "ro"]);
        }
        args.push(source.to_str().unwrap_or_default());
        args.push(target.to_str().unwrap_or_default());

        let output = std::process::Command::new("mount")
            .args(&args)
            .output()?;

        if !output.status.success() {
            return Err(MountError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Unmount a filesystem.
    pub fn unmount(target: &Path) -> Result<(), MountError> {
        let output = std::process::Command::new("umount")
            .arg(target.to_str().unwrap_or_default())
            .output()?;

        if !output.status.success() {
            return Err(MountError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Force unmount.
    pub fn force_unmount(target: &Path) -> Result<(), MountError> {
        let output = std::process::Command::new("umount")
            .args(["-f", target.to_str().unwrap_or_default()])
            .output()?;

        if !output.status.success() {
            return Err(MountError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    /// Mount devfs inside a jail root.
    pub fn mount_devfs(jail_root: &Path) -> Result<(), MountError> {
        let devfs_path = jail_root.join("dev");
        std::fs::create_dir_all(&devfs_path).map_err(MountError::Io)?;

        let output = std::process::Command::new("mount")
            .args(["-t", "devfs", "devfs", devfs_path.to_str().unwrap_or_default()])
            .output()?;

        if !output.status.success() {
            return Err(MountError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }
}
