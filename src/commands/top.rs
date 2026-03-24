use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Args;
use clap_complete::engine::ArgValueCompleter;

use crate::completions;
use crate::freebsd::ps;
use crate::freebsd::zfs::Zfs;
use crate::jail::config::GlobalConfig;
use crate::jail::lifecycle::JailLifecycle;
use crate::jail::state::JailStatus;
use crate::output::Table;

static RUNNING: AtomicBool = AtomicBool::new(true);

#[derive(Args)]
#[command(after_long_help = "\
Examples:
  dail top                     Watch all running jails (refresh every 2s)
  dail top postgres            Watch a single jail
  dail top --interval 5        Refresh every 5 seconds
  dail top --once              Print once and exit")]
pub struct TopArgs {
    /// Filter by jail name
    #[arg(add = ArgValueCompleter::new(completions::complete_running_jail_names))]
    pub name: Option<String>,

    /// Refresh interval in seconds
    #[arg(short, long, default_value = "2")]
    pub interval: u64,

    /// Print once and exit (no watch loop)
    #[arg(long)]
    pub once: bool,
}

pub fn run(args: TopArgs) -> anyhow::Result<()> {
    let watch = !args.once && std::io::stdout().is_terminal();

    if watch {
        // SAFETY: handler only sets an atomic bool, async-signal-safe
        unsafe {
            libc::signal(libc::SIGINT, signal_handler as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, signal_handler as *const () as libc::sighandler_t);
        }
    }

    loop {
        if watch {
            print!("\x1b[2J\x1b[H");
        }

        print_top(&args)?;

        if !watch || !RUNNING.load(Ordering::Relaxed) {
            break;
        }

        std::thread::sleep(std::time::Duration::from_secs(args.interval));
    }

    Ok(())
}

extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::Relaxed);
}

fn print_top(args: &TopArgs) -> anyhow::Result<()> {
    let global = GlobalConfig::load()?;
    let lifecycle = JailLifecycle::new_readonly(global.clone())?;

    let jails: Vec<_> = lifecycle
        .list()
        .into_iter()
        .filter(|j| j.status == JailStatus::Running)
        .filter(|j| {
            args.name
                .as_ref()
                .map(|n| j.name() == n)
                .unwrap_or(true)
        })
        .collect();

    if jails.is_empty() {
        if let Some(ref name) = args.name {
            println!("Jail '{name}' is not running.");
        } else {
            println!("No running jails.");
        }
        return Ok(());
    }

    let headers = if args.once {
        vec!["NAME", "JID", "CPU%", "MEM", "NPROC", "DISK", "NET I/O", "UPTIME"]
    } else {
        vec!["NAME", "JID", "CPU%", "MEM", "NPROC", "DISK", "NET I/O", "CONN", "UPTIME"]
    };
    let mut table = Table::new(headers);

    for jail in &jails {
        let jid = jail.jid.unwrap_or(0);
        let usage = ps::jail_usage(jid);

        let cpu = if usage.nproc > 0 {
            format!("{:.1}%", usage.cpu_percent)
        } else {
            "-".to_string()
        };

        let mem = if usage.nproc > 0 {
            format_mem_kb(usage.mem_kb)
        } else {
            "-".to_string()
        };

        let nproc = if usage.nproc > 0 {
            usage.nproc.to_string()
        } else {
            "-".to_string()
        };

        let disk = if global.storage_backend == "zfs" {
            if let Ok(pool) = global.zfs_pool.as_deref().ok_or(()) {
                let dataset = format!("{pool}/dail/jails/{}", jail.name());
                Zfs::get_property(&dataset, "used")
                    .unwrap_or_else(|_| "-".to_string())
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        };

        let net_io = jail_net_io(jid);

        let uptime = jail
            .started_at
            .map(|t| format_duration(chrono::Utc::now() - t))
            .unwrap_or_else(|| "-".to_string());

        let mut row = vec![
            jail.name().to_string(),
            jid.to_string(),
            cpu,
            mem,
            nproc,
            disk,
            net_io,
        ];
        if !args.once {
            row.push(jail_conn_count(jid));
        }
        row.push(uptime);
        table.add_row(row);
    }

    table.print();
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}G", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{}K", bytes / 1024)
    } else {
        format!("{}B", bytes)
    }
}

/// Get network I/O for a jail via netstat -j (works for both vnet and inherit).
fn jail_net_io(jid: i32) -> String {
    // netstat -j <jid> -bI gives per-interface byte counters
    // For inherit jails this won't show jail-specific traffic, so fall back to "-"
    let output = std::process::Command::new("netstat")
        .args(["-j", &jid.to_string(), "-bn", "-f", "inet", "--libxo", "json"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return "-".to_string(),
    };

    // Parse JSON output for rx/tx bytes from statistics
    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return "-".to_string(),
    };

    let mut rx: u64 = 0;
    let mut tx: u64 = 0;

    if let Some(sockets) = json.pointer("/statistics/socket").and_then(|v| v.as_array()) {
        for sock in sockets {
            rx += sock.get("received-bytes").and_then(|v| v.as_u64()).unwrap_or(0);
            tx += sock.get("sent-bytes").and_then(|v| v.as_u64()).unwrap_or(0);
        }
    }

    if rx == 0 && tx == 0 {
        return "-".to_string();
    }

    format!("{}/{}", format_bytes(rx), format_bytes(tx))
}

/// Count active TCP connections for a jail via sockstat.
fn jail_conn_count(jid: i32) -> String {
    let output = std::process::Command::new("sockstat")
        .args(["-j", &jid.to_string(), "-c", "-4", "-6", "-q", "-P", "tcp"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let count = String::from_utf8_lossy(&o.stdout)
                .lines()
                .count();
            count.to_string()
        }
        _ => "-".to_string(),
    }
}

fn format_mem_kb(kb: u64) -> String {
    if kb >= 1_048_576 {
        format!("{:.1}G", kb as f64 / 1_048_576.0)
    } else if kb >= 1024 {
        format!("{}M", kb / 1024)
    } else {
        format!("{}K", kb)
    }
}

fn format_duration(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds().max(0);
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {:02}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}
