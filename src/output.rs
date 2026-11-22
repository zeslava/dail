use crate::jail::state::{JailState, JailStatus};
use crate::jail::config::JailConfig;
use crate::network::NetworkConfig;
use std::io::IsTerminal;

fn colored_status(status: &JailStatus) -> String {
    let s = status.to_string();
    if !std::io::stdout().is_terminal() {
        return s;
    }
    match status {
        JailStatus::Running => format!("\x1b[32m{s}\x1b[0m"),  // green
        JailStatus::Idle => format!("\x1b[33m{s}\x1b[0m"),     // yellow
        JailStatus::Stopped => format!("\x1b[31m{s}\x1b[0m"),  // red
        JailStatus::Created => format!("\x1b[33m{s}\x1b[0m"),  // yellow
    }
}

/// Auto-width table helper: computes max column widths from data.
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<&str>) -> Self {
        Self {
            headers: headers.into_iter().map(String::from).collect(),
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn print(&self) {
        if self.rows.is_empty() {
            return;
        }

        let widths: Vec<usize> = (0..self.headers.len())
            .map(|i| {
                let header_w = self.headers[i].len();
                let max_data = self.rows.iter()
                    .map(|r| r.get(i).map(|s| strip_ansi(s).len()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                header_w.max(max_data) + 2
            })
            .collect();

        // Header
        let header: String = self.headers.iter().zip(&widths)
            .map(|(h, w)| format!("{h:<w$}"))
            .collect();
        println!("{header}");
        let total: usize = widths.iter().sum();
        println!("{}", "-".repeat(total));

        // Rows
        for row in &self.rows {
            let line: String = row.iter().zip(&widths)
                .map(|(val, w)| {
                    let visible = strip_ansi(val).len();
                    let padding = if *w > visible { w - visible } else { 0 };
                    format!("{val}{}", " ".repeat(padding))
                })
                .collect();
            println!("{line}");
        }
    }
}

fn format_duration(d: chrono::Duration) -> String {
    let secs = d.num_seconds();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn format_inherited_ports(config: &JailConfig, listeners: Option<Vec<String>>) -> String {
    if let Some(ports) = listeners {
        return if ports.is_empty() {
            "-".to_string()
        } else {
            ports.join(", ")
        };
    }

    // Configuration is only a hint when the jail is stopped or inspection fails.
    if let Some(port) = config
        .env
        .get("PORT")
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|&port| port != 0)
    {
        return format!("{port} (env)");
    }

    let ports: std::collections::BTreeSet<_> = config
        .ports
        .iter()
        .map(|p| (p.jail_port, p.proto.as_str()))
        .collect();
    if ports.is_empty() {
        "-".to_string()
    } else {
        let ports = ports
            .into_iter()
            .map(|(port, proto)| format!("{port}/{proto}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{ports} (config)")
    }
}

fn format_ports(jail: &JailState) -> String {
    match &jail.config.network {
        NetworkConfig::Alias { .. } => jail
            .config
            .ports
            .iter()
            .map(|p| format!("{}->{}/{}", p.host_port, p.jail_port, p.proto))
            .collect::<Vec<_>>()
            .join(", "),
        NetworkConfig::Inherit => {
            let listeners = if matches!(jail.status, JailStatus::Running | JailStatus::Idle) {
                jail.jid.and_then(crate::freebsd::ps::jail_listening_ports)
            } else {
                None
            };
            format_inherited_ports(&jail.config, listeners)
        }
        NetworkConfig::Vnet { .. } | NetworkConfig::None => "-".to_string(),
    }
}

pub fn print_table(jails: &[&JailState]) {
    if jails.is_empty() {
        println!("No jails found.");
        return;
    }

    let mut table = Table::new(vec!["NAME", "STATUS", "JID", "IP", "PORTS", "BASE", "TYPE", "UPTIME"]);

    for jail in jails {
        let jid = jail.jid.map(|j| j.to_string()).unwrap_or_else(|| "-".to_string());
        let base = jail.config.base.as_deref().unwrap_or("-").to_string();
        let uptime = match jail.status {
            JailStatus::Running | JailStatus::Idle => {
                jail.started_at
                    .map(|t| format_duration(chrono::Utc::now() - t))
                    .unwrap_or_else(|| "-".to_string())
            }
            _ => "-".to_string(),
        };
        let jail_type = format!("{:?}", jail.config.jail_type).to_lowercase();
        let ip = match &jail.config.network {
            NetworkConfig::Alias { ip, .. } => ip.clone(),
            NetworkConfig::Vnet { ip, .. } => ip.clone(),
            NetworkConfig::Inherit => "inherit".to_string(),
            NetworkConfig::None => "none".to_string(),
        };

        let ports = format_ports(jail);

        table.add_row(vec![
            jail.name().to_string(),
            colored_status(&jail.status),
            jid,
            ip,
            ports,
            base,
            jail_type,
            uptime,
        ]);
    }

    table.print();
}

pub fn print_names(jails: &[&JailState]) {
    for jail in jails {
        println!("{}", jail.name());
    }
}

pub fn print_json(jails: &[&JailState]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&jails)?;
    println!("{json}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jail::config::PortMapping;

    fn demo_config() -> JailConfig {
        let mut config = JailConfig::new("aurora-demo".into());
        config.env.insert("PORT".into(), "4001".into());
        config.ports.push(PortMapping {
            host_ip: None,
            host_port: 4000,
            jail_port: 4000,
            proto: "tcp".into(),
        });
        config
    }

    #[test]
    fn observed_ports_override_configuration_and_empty_result_is_authoritative() {
        let config = demo_config();
        assert_eq!(
            format_inherited_ports(&config, Some(vec!["5000/tcp".into()])),
            "5000/tcp"
        );
        assert_eq!(format_inherited_ports(&config, Some(vec![])), "-");
    }

    #[test]
    fn failed_inspection_prefers_valid_port_env_and_labels_the_hint() {
        let mut config = demo_config();
        assert_eq!(format_inherited_ports(&config, None), "4001 (env)");
        for value in ["", "0", "65536", "not-a-port"] {
            config.env.insert("PORT".into(), value.into());
            assert_eq!(format_inherited_ports(&config, None), "4000/tcp (config)");
        }
        config.env.clear();
        config.ports.clear();
        assert_eq!(format_inherited_ports(&config, None), "-");
    }

    #[test]
    fn stopped_inherit_uses_hints_and_alias_retains_forwarding_notation() {
        let mut jail = JailState::new(demo_config(), std::path::PathBuf::from("/unused"));
        jail.status = JailStatus::Stopped;
        // A stopped jail must not probe any retained JID.
        jail.jid = Some(12345);
        assert_eq!(format_ports(&jail), "4001 (env)");
        jail.config.network = NetworkConfig::Alias {
            ip: "10.100.0.11/24".into(),
            interface: "lo0".into(),
        };
        assert_eq!(format_ports(&jail), "4000->4000/tcp");
        jail.config.network = NetworkConfig::None;
        assert_eq!(format_ports(&jail), "-");
    }
}
