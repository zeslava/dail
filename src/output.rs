use crate::jail::state::{JailState, JailStatus};
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

pub fn print_table(jails: &[&JailState]) {
    if jails.is_empty() {
        println!("No jails found.");
        return;
    }

    let mut table = Table::new(vec!["NAME", "STATUS", "JID", "IP", "BASE", "TYPE", "CREATED"]);

    for jail in jails {
        let jid = jail.jid.map(|j| j.to_string()).unwrap_or_else(|| "-".to_string());
        let base = jail.config.base.as_deref().unwrap_or("-").to_string();
        let created = jail.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let jail_type = format!("{:?}", jail.config.jail_type).to_lowercase();
        let ip = match &jail.config.network {
            NetworkConfig::Alias { ip, .. } => ip.clone(),
            NetworkConfig::Vnet { ip, .. } => ip.clone(),
            NetworkConfig::Inherit => "inherit".to_string(),
            NetworkConfig::None => "none".to_string(),
        };

        table.add_row(vec![
            jail.name().to_string(),
            colored_status(&jail.status),
            jid,
            ip,
            base,
            jail_type,
            created,
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
