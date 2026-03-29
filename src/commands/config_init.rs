use crate::jail::config::{GlobalConfig, StorageKind};

pub fn run(zfs_pool: Option<String>) -> anyhow::Result<()> {
    let mut config = GlobalConfig::default();

    if let Some(pool) = zfs_pool {
        config.storage_backend = StorageKind::Zfs;
        config.zfs_pool = Some(pool);
    }

    config.init()?;
    println!("Dail initialized at {}", config.root_dir.display());

    if config.storage_backend == StorageKind::Zfs {
        println!(
            "Storage backend: ZFS (pool: {})",
            config.zfs_pool.as_deref().unwrap_or("?")
        );
    } else {
        println!("Storage backend: directory");
    }

    println!("\nNext step: dail bootstrap");

    Ok(())
}
