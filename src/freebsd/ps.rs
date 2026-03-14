use std::process::Command;

#[derive(Debug, Default)]
pub struct JailResourceUsage {
    pub cpu_percent: f64,
    pub mem_kb: u64,
    pub nproc: u32,
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
        .args(["-o", "pcpu=,rss=", "-J", &jid.to_string()])
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
            nproc += 1;
        }
    }

    JailResourceUsage {
        cpu_percent: total_cpu,
        mem_kb: total_rss,
        nproc,
    }
}
