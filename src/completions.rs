use clap_complete::engine::CompletionCandidate;

use crate::image::ImageStore;
use crate::jail::config::GlobalConfig;
use crate::jail::preset::Preset;
use crate::jail::state::JailStatus;
use crate::storage;
use crate::store::DailStore;

fn load_jail_names(filter: Option<JailStatus>) -> Vec<CompletionCandidate> {
    let Ok(config) = GlobalConfig::load() else {
        return Vec::new();
    };
    let Ok(store) = DailStore::new(&config) else {
        return Vec::new();
    };
    store
        .list()
        .into_iter()
        .filter(|j| filter.is_none_or(|s| j.status == s))
        .map(|j| CompletionCandidate::new(j.config.name.as_str()).display_order(Some(0)))
        .collect()
}

pub fn complete_jail_names(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    load_jail_names(None)
}

pub fn complete_running_jail_names(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    load_jail_names(Some(JailStatus::Running))
}

pub fn complete_image_refs(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Ok(config) = GlobalConfig::load() else {
        return Vec::new();
    };
    let store = ImageStore::new(&config);
    let Ok(images) = store.list() else {
        return Vec::new();
    };
    images
        .into_iter()
        .map(|m| CompletionCandidate::new(format!("{}:{}", m.name, m.tag)))
        .collect()
}

pub fn complete_preset_names(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Ok(config) = GlobalConfig::load() else {
        return Vec::new();
    };
    Preset::list_all(&config)
        .into_iter()
        .map(|p| CompletionCandidate::new(p.name))
        .collect()
}

pub fn complete_jail_type(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate::new("thick"),
        CompletionCandidate::new("thin"),
    ]
}

pub fn complete_network_mode(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    vec![
        CompletionCandidate::new("inherit"),
        CompletionCandidate::new("none"),
    ]
}

pub fn complete_base_releases(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let Ok(config) = GlobalConfig::load() else {
        return Vec::new();
    };
    let backend = storage::get_backend(&config);
    let Ok(bases) = backend.list_bases(&config) else {
        return Vec::new();
    };
    bases
        .into_iter()
        .map(|(name, _)| CompletionCandidate::new(name))
        .collect()
}
