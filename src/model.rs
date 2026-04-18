use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordEntry {
    pub id: String,
    pub service: String,
    pub username: String,
    pub secret: String,
    pub notes: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub last_event_id: Option<String>,
}

impl PasswordEntry {
    pub fn merge_prefer_newer(current: Option<&Self>, incoming: Self) -> Self {
        match current {
            Some(existing) if existing.updated_at >= incoming.updated_at => existing.clone(),
            _ => incoming,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordEnvelope {
    pub schema: String,
    pub entry: PasswordEntry,
}

impl PasswordEnvelope {
    pub fn from_entry(entry: PasswordEntry) -> Self {
        Self {
            schema: "niplock.v1".to_string(),
            entry,
        }
    }
}
