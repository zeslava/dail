#[derive(Debug, thiserror::Error)]
pub enum IfconfigError {
    #[error("ifconfig failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

/// An epair interface pair (e.g., epair0a / epair0b).
#[derive(Debug, Clone)]
pub struct EpairPair {
    pub host_side: String,
    pub jail_side: String,
}

pub struct Ifconfig;

impl Ifconfig {
    /// Add an IP alias to an interface.
    pub fn add_alias(interface: &str, addr: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[interface, "inet", addr, "alias"])
    }

    /// Remove an IP alias from an interface.
    pub fn remove_alias(interface: &str, addr: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[interface, "inet", addr, "-alias"])
    }

    /// Create an epair interface pair.
    pub fn create_epair() -> Result<EpairPair, IfconfigError> {
        let output = std::process::Command::new("ifconfig")
            .args(["epair", "create"])
            .output()?;

        if !output.status.success() {
            return Err(IfconfigError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let host_side = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // epair creates e.g. "epair0a", the other side is "epair0b"
        let jail_side = if host_side.ends_with('a') {
            format!("{}b", &host_side[..host_side.len() - 1])
        } else {
            return Err(IfconfigError::Parse(format!(
                "unexpected epair name: {host_side}"
            )));
        };

        Ok(EpairPair {
            host_side,
            jail_side,
        })
    }

    /// Destroy an interface.
    pub fn destroy(interface: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[interface, "destroy"])
    }

    /// Add an interface to a bridge.
    pub fn bridge_add(bridge: &str, member: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[bridge, "addm", member])
    }

    /// Remove an interface from a bridge.
    pub fn bridge_remove(bridge: &str, member: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[bridge, "deletem", member])
    }

    /// Bring an interface up.
    pub fn up(interface: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[interface, "up"])
    }

    /// Move interface into a vnet jail.
    pub fn vnet_move(interface: &str, jail_name: &str) -> Result<(), IfconfigError> {
        run_ifconfig(&[interface, "vnet", jail_name])
    }
}

fn run_ifconfig(args: &[&str]) -> Result<(), IfconfigError> {
    let output = std::process::Command::new("ifconfig")
        .args(args)
        .output()?;

    if !output.status.success() {
        return Err(IfconfigError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(())
}
