use serde::{Deserialize, Serialize};

use crate::error::DailError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum NetworkConfig {
    Alias {
        ip: String,
        #[serde(default = "default_interface")]
        interface: String,
    },
    Vnet {
        #[serde(default = "default_bridge")]
        bridge: String,
        ip: String,
        #[serde(default)]
        gateway: Option<String>,
    },
    #[default]
    Inherit,
    None,
}

fn default_interface() -> String {
    "lo0".to_string()
}

fn default_bridge() -> String {
    "bridge0".to_string()
}

/// Set up networking for a jail. Returns the epair base name (e.g. "epair0") for Vnet.
pub fn setup_network(jail_name: &str, config: &NetworkConfig) -> Result<Option<String>, DailError> {
    use crate::freebsd::ifconfig::Ifconfig;
    use crate::freebsd::jail_sys::JailSys;

    match config {
        NetworkConfig::Alias { ip, interface } => {
            Ifconfig::add_alias(interface, ip).map_err(|e| DailError::Network(e.to_string()))?;
            Ok(None)
        }
        NetworkConfig::Vnet {
            bridge,
            ip,
            gateway,
        } => {
            let pair = Ifconfig::create_epair().map_err(|e| DailError::Network(e.to_string()))?;

            // Extract base name (e.g. "epair0" from "epair0a")
            let epair_base = pair
                .host_side
                .strip_suffix('a')
                .unwrap_or(&pair.host_side)
                .to_string();

            Ifconfig::bridge_add(bridge, &pair.host_side)
                .map_err(|e| DailError::Network(e.to_string()))?;
            Ifconfig::up(&pair.host_side).map_err(|e| DailError::Network(e.to_string()))?;

            Ifconfig::vnet_move(&pair.jail_side, jail_name)
                .map_err(|e| DailError::Network(e.to_string()))?;

            JailSys::exec(jail_name, &["ifconfig", &pair.jail_side, ip])
                .map_err(|e| DailError::Network(e.to_string()))?;

            if let Some(gw) = gateway {
                JailSys::exec(jail_name, &["route", "add", "default", gw])
                    .map_err(|e| DailError::Network(e.to_string()))?;
            }

            Ok(Some(epair_base))
        }
        NetworkConfig::Inherit | NetworkConfig::None => Ok(None),
    }
}

/// Parse "a.b.c.d/prefix" and return the next free host IP not in `used`.
/// Iterates from .2 to .254.
pub fn next_free_ip(subnet: &str, used: &[String]) -> Option<String> {
    let (addr_part, prefix_part) = subnet.split_once('/')?;
    let prefix: u8 = prefix_part.parse().ok()?;

    let octets: Vec<u8> = addr_part
        .split('.')
        .map(|o| o.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    if octets.len() != 4 {
        return None;
    }

    let base = u32::from_be_bytes([octets[0], octets[1], octets[2], octets[3]]);
    let mask = if prefix == 0 {
        0u32
    } else {
        !0u32 << (32 - prefix)
    };
    let network = base & mask;

    for host in 2u32..=254 {
        let ip_u32 = network | host;
        let [a, b, c, d] = ip_u32.to_be_bytes();
        let candidate = format!("{a}.{b}.{c}.{d}/{prefix}");
        let candidate_bare = format!("{a}.{b}.{c}.{d}");
        if !used.iter().any(|u| {
            let u_bare = u.split('/').next().unwrap_or(u);
            u_bare == candidate_bare || *u == candidate
        }) {
            return Some(candidate);
        }
    }
    None
}

pub fn teardown_network(
    _jail_name: &str,
    config: &NetworkConfig,
    epair: Option<&str>,
) -> Result<(), DailError> {
    use crate::freebsd::ifconfig::Ifconfig;

    match config {
        NetworkConfig::Alias { ip, interface } => {
            // Alias may already be gone (e.g. jail removed it on stop)
            let _ = Ifconfig::remove_alias(interface, ip);
        }
        NetworkConfig::Vnet { .. } => {
            if let Some(epair_base) = epair {
                // Destroy host side — this also removes the jail side
                let host_iface = format!("{epair_base}a");
                let _ = Ifconfig::destroy(&host_iface);
            }
        }
        NetworkConfig::Inherit | NetworkConfig::None => {}
    }
    Ok(())
}
