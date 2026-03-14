use crate::jail::config::PortMapping;
use std::process::Command;

/// Get the default route interface name.
fn default_interface() -> Option<String> {
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(iface) = line.strip_prefix("interface:") {
            return Some(iface.trim().to_string());
        }
    }
    None
}

/// Check if /etc/pf.conf contains the required rdr-anchor for dail.
fn check_pf_anchor() {
    let content = match std::fs::read_to_string("/etc/pf.conf") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "warning: /etc/pf.conf not found — PF is not configured.\n\
                 Port forwarding requires PF. Enable it in /etc/rc.conf:\n\
                 \n  pf_enable=\"YES\"\n\
                 \nThen create /etc/pf.conf with at least:\n\
                 \n  rdr-anchor \"dail/*\"\n"
            );
            return;
        }
        Err(_) => {
            eprintln!("warning: cannot read /etc/pf.conf — port forwarding may not work.");
            return;
        }
    };

    let has_anchor = content.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.contains("rdr-anchor") && trimmed.contains("dail")
    });

    if !has_anchor {
        eprintln!(
            "warning: /etc/pf.conf is missing the dail rdr-anchor. Port forwarding will not work.\n\
             Add the following line to /etc/pf.conf and reload (`pfctl -f /etc/pf.conf`):\n\
             \n  rdr-anchor \"dail/*\"\n"
        );
    }
}

/// Write PF rdr rules into the `dail/<jail_name>` anchor.
pub fn setup_port_forwarding(jail_name: &str, jail_ip: &str, ports: &[PortMapping]) {
    if ports.is_empty() {
        return;
    }

    check_pf_anchor();

    let iface = match default_interface() {
        Some(i) => i,
        None => {
            eprintln!("warning: could not determine default interface, skipping port forwarding");
            return;
        }
    };

    // Strip CIDR suffix from jail IP (e.g. 10.100.0.5/24 -> 10.100.0.5)
    let ip = jail_ip.split('/').next().unwrap_or(jail_ip);

    let mut rules = String::new();
    for p in ports {
        let on = if let Some(ref host_ip) = p.host_ip {
            format!("on {{ {} }}", host_ip)
        } else {
            format!("on {}", iface)
        };
        rules.push_str(&format!(
            "rdr pass {} proto {} from any to any port {} -> {} port {}\n",
            on, p.proto, p.host_port, ip, p.jail_port
        ));
    }

    let anchor = format!("dail/{}", jail_name);
    let mut child = match Command::new("pfctl")
        .args(["-a", &anchor, "-f", "-"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to run pfctl for port forwarding: {e}");
            return;
        }
    };

    if let Some(ref mut stdin) = child.stdin {
        use std::io::Write;
        let _ = stdin.write_all(rules.as_bytes());
    }

    match child.wait() {
        Ok(status) if !status.success() => {
            eprintln!("warning: pfctl failed to load port forwarding rules for '{jail_name}'");
        }
        Err(e) => {
            eprintln!("warning: pfctl error: {e}");
        }
        _ => {
            tracing::info!("Port forwarding configured for '{}': {} rule(s)", jail_name, ports.len());
        }
    }
}

/// Flush all PF rules from the `dail/<jail_name>` anchor.
pub fn teardown_port_forwarding(jail_name: &str) {
    let anchor = format!("dail/{}", jail_name);
    match Command::new("pfctl")
        .args(["-a", &anchor, "-F", "all"])
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't warn if anchor simply doesn't exist
            if !stderr.contains("does not exist") {
                tracing::warn!("pfctl flush for '{}': {}", jail_name, stderr.trim());
            }
        }
        Err(e) => {
            tracing::warn!("pfctl teardown for '{}': {}", jail_name, e);
        }
        _ => {}
    }
}
