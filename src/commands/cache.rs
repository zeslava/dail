use crate::jail::config::GlobalConfig;

pub fn clean() -> anyhow::Result<()> {
    let config = GlobalConfig::load()?;
    let cache = config.cache_dir();
    if cache.exists() {
        std::fs::remove_dir_all(&cache)?;
        println!("Cache cleaned: {}", cache.display());
    } else {
        println!("Nothing to clean");
    }
    Ok(())
}
