use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use super::config::JailConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JailStatus {
    Created,
    Running,
    Stopped,
}

impl std::fmt::Display for JailStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JailStatus::Created => write!(f, "created"),
            JailStatus::Running => write!(f, "running"),
            JailStatus::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JailState {
    pub id: String,
    pub config: JailConfig,
    pub status: JailStatus,
    pub jid: Option<i32>,
    pub root_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub epair: Option<String>,
}

impl JailState {
    pub fn new(config: JailConfig, root_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            status: JailStatus::Created,
            jid: None,
            root_path,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            epair: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn is_stopped(&self) -> bool {
        self.status == JailStatus::Stopped
    }
}
