use clap_complete::engine::CompletionCandidate;

use crate::image::ImageStore;
use crate::jail::config::GlobalConfig;
use crate::jail::preset::Preset;
use crate::jail::state::JailStatus;
use crate::storage;
use crate::store::DailStore;

fn load_jail_names(filter: Option<&[JailStatus]>) -> Vec<CompletionCandidate> {
    let Ok(config) = GlobalConfig::load() else {
        return Vec::new();
    };
    let Ok(store) = DailStore::new(&config) else {
        return Vec::new();
    };
    let mut jails = store.list();
    if let Some(statuses) = filter {
        jails.retain(|j| statuses.contains(&j.status));
    }
    jails.sort_by(|a, b| a.config.name.cmp(&b.config.name));
    jails
        .into_iter()
        .enumerate()
        .map(|(i, j)| CompletionCandidate::new(j.config.name.clone()).display_order(Some(i)))
        .collect()
}

pub fn complete_jail_names(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    load_jail_names(None)
}

pub fn complete_running_jail_names(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    load_jail_names(Some(&[JailStatus::Running, JailStatus::Idle]))
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

/// Complete first positional arg of `dail run`: jail names + .dail file paths
pub fn complete_run_first(_current: &std::ffi::OsStr) -> Vec<CompletionCandidate> {
    let mut candidates = load_jail_names(None);

    // Add .dail files from current directory
    if let Ok(entries) = std::fs::read_dir(".") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if name_str.ends_with(".dail") {
                candidates.push(CompletionCandidate::new(name_str));
            }
        }
    }

    candidates
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
