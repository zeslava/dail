use crate::jail::config::GlobalConfig;

pub fn run() -> anyhow::Result<()> {
    let config = GlobalConfig::load()?;

    let yaml_path = std::path::Path::new("/usr/local/etc/dail/config.yaml");
    let toml_path = std::path::Path::new("/usr/local/etc/dail/config.toml");

    let config_file = if yaml_path.exists() {
        yaml_path.to_str().unwrap_or("config.yaml")
    } else if toml_path.exists() {
        toml_path.to_str().unwrap_or("config.toml")
    } else {
        "(defaults, no config file)"
    };

    println!("Config file:      {config_file}");
    println!("Root directory:   {}", config.root_dir.display());
    println!("Storage backend:  {}", config.storage_backend);
    if let Some(ref pool) = config.zfs_pool {
        println!("ZFS pool:         {pool}");
    }
    println!("Default network:  {:?}", config.default_network);
    println!("Alias interface:  {}", config.alias_interface);
    println!("IP pool:          {}", config.ip_pool);
    println!("Mirror:           {}", config.mirror);
    println!("Default base:     {}", config.default_base);

    Ok(())
}
