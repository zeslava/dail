use std::collections::BTreeSet;
use std::process::Command;

/// Inspect IPv4/IPv6 listeners owned by a jail, including inherited networking.
/// None means inspection failed; an empty list means no visible listeners.
pub fn jail_listening_ports(jid: i32) -> Option<Vec<String>> {
    // JID 0 selects the host, so never probe a missing or invalid jail ID.
    if jid <= 0 {
        return None;
    }
    let output = Command::new("sockstat")
        .args(["-46ln", "-j", &jid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_listening_ports(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_listening_ports(stdout: &str) -> Vec<String> {
    let mut ports = BTreeSet::new();
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        // USER COMMAND PID FD PROTO LOCAL-ADDRESS FOREIGN-ADDRESS
        let protocol = match fields.nth(4) {
            Some("tcp4" | "tcp6" | "tcp46") => "tcp",
            Some("udp4" | "udp6" | "udp46") => "udp",
            _ => continue,
        };
        let Some(local) = fields.next() else { continue };
        if fields.next() != Some("*:*") {
            continue;
        }
        let Some(port) = local
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse::<u16>().ok())
            .filter(|&port| port != 0)
        else {
            continue;
        };
        ports.insert((port, protocol));
    }
    ports
        .into_iter()
        .map(|(port, protocol)| format!("{port}/{protocol}"))
        .collect()
}

#[derive(Debug, Default)]
pub struct JailResourceUsage {
    /// Decayed average from `ps -o pcpu` — fallback only, see `cpu_time_secs`.
    pub cpu_percent: f64,
    /// Accumulated CPU seconds of all processes; diff two samples for real load.
    pub cpu_time_secs: f64,
    pub mem_kb: u64,
    pub nproc: u32,
}

/// Parse FreeBSD ps `time` field: `[[dd-]hh:]mm:ss.ss`.
fn parse_cputime(s: &str) -> Option<f64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, s),
    };
    let mut secs = 0.0;
    for part in rest.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(days * 86400.0 + secs)
}

/// Check if a jail has any running processes.
pub fn jail_has_processes(jid: i32) -> bool {
    Command::new("ps")
        .args(["-o", "pid=", "-J", &jid.to_string()])
        .output()
        .map(|o| o.status.success() && o.stdout.iter().any(|&b| b.is_ascii_digit()))
        .unwrap_or(false)
}

/// Collect aggregate CPU%, RSS, and process count for a jail by JID.
/// Uses `ps -o pcpu=,rss= -J <jid>` — works without root.
/// Returns default (zeros) if the jail has no processes or JID is stale.
pub fn jail_usage(jid: i32) -> JailResourceUsage {
    let output = match Command::new("ps")
        .args([
            "-o",
            "pcpu=",
            "-o",
            "rss=",
            "-o",
            "time=",
            "-J",
            &jid.to_string(),
        ])
        .output()
    {
        Ok(o) => o,
        Err(_) => return JailResourceUsage::default(),
    };

    if !output.status.success() {
        return JailResourceUsage::default();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut total_cpu = 0.0f64;
    let mut total_rss = 0u64;
    let mut total_time = 0.0f64;
    let mut nproc = 0u32;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(cpu) = parts[0].parse::<f64>() {
                total_cpu += cpu;
            }
            if let Ok(rss) = parts[1].parse::<u64>() {
                total_rss += rss;
            }
            if let Some(t) = parts.get(2).and_then(|p| parse_cputime(p)) {
                total_time += t;
            }
            nproc += 1;
        }
    }

    JailResourceUsage {
        cpu_percent: total_cpu,
        cpu_time_secs: total_time,
        mem_kb: total_rss,
        nproc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sockstat_listeners_and_deduplicates_families_and_workers() {
        let output = "USER COMMAND PID FD PROTO LOCAL ADDRESS FOREIGN ADDRESS
root aurora 16427 12 tcp4 *:4001 *:*
root aurora 16427 13 tcp6 :::4001 *:*
root worker 16428 12 tcp4 *:4001 *:*
root dns 200 3 udp4 127.0.0.1:53 *:*
root dns 200 4 tcp6 ::1:53 *:*
root app 300 5 tcp46 *:8080 *:*
root app 300 6 udp6 fe80::1%lo0:8080 *:*
root app 300 7 tcp4 192.168.1.100:51234 203.0.113.1:443
";
        assert_eq!(
            parse_listening_ports(output),
            ["53/tcp", "53/udp", "4001/tcp", "8080/tcp", "8080/udp"]
        );
    }

    #[test]
    fn ignores_headers_unix_sockets_and_invalid_ports() {
        let output = "USER COMMAND PID FD PROTO LOCAL ADDRESS FOREIGN ADDRESS
root app 1 3 stream /tmp/app.sock
root app 1 4 tcp4 *:* *:*
root app 1 5 tcp4 *:0 *:*
root app 1 6 tcp4 *:65536 *:*
root app 1 7 tcp4 no-address *:*
root app 1 8 tcp4
garbage
";
        assert!(parse_listening_ports(output).is_empty());
        assert!(parse_listening_ports("").is_empty());
    }

    #[test]
    fn never_queries_host_sockets_for_invalid_jid() {
        assert_eq!(jail_listening_ports(0), None);
        assert_eq!(jail_listening_ports(-1), None);
    }
}
