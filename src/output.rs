use crate::jail::state::JailState;
use crate::network::NetworkConfig;

pub fn print_table(jails: &[&JailState]) {
    if jails.is_empty() {
        println!("No jails found.");
        return;
    }

    println!(
        "{:<20} {:<8} {:<12} {:<10} {:<20} {:<20} {:<24}",
        "NAME", "TYPE", "STATUS", "JID", "IP", "BASE", "CREATED"
    );
    println!("{}", "-".repeat(114));

    for jail in jails {
        let jid = jail
            .jid
            .map(|j| j.to_string())
            .unwrap_or_else(|| "-".to_string());
        let base = jail.config.base.as_deref().unwrap_or("-");
        let created = jail.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let jail_type = format!("{:?}", jail.config.jail_type).to_lowercase();
        let ip = match &jail.config.network {
            NetworkConfig::Alias { ip, .. } => ip.as_str(),
            NetworkConfig::Vnet { ip, .. } => ip.as_str(),
            NetworkConfig::Inherit => "inherit",
            NetworkConfig::None => "none",
        };

        println!(
            "{:<20} {:<8} {:<12} {:<10} {:<20} {:<20} {:<24}",
            jail.name(),
            jail_type,
            jail.status.to_string(),
            jid,
            ip,
            base,
            created,
        );
    }
}

pub fn print_json(jails: &[&JailState]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(&jails)?;
    println!("{json}");
    Ok(())
}
