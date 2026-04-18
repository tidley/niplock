use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::PasswordEntry;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultSnapshot {
    pub entries: HashMap<String, PasswordEntry>,
    pub last_sync_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct LocalStore {
    path: PathBuf,
}

impl LocalStore {
    pub fn new() -> Result<Self> {
        let base = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("niplock");
        fs::create_dir_all(&base)?;

        Ok(Self {
            path: base.join("vault.json"),
        })
    }

    pub fn load(&self) -> Result<VaultSnapshot> {
        if !self.path.exists() {
            return Ok(VaultSnapshot::default());
        }

        let content = fs::read_to_string(&self.path)?;
        let snapshot = serde_json::from_str(&content)?;
        Ok(snapshot)
    }

    pub fn save(&self, snapshot: &VaultSnapshot) -> Result<()> {
        let content = serde_json::to_string_pretty(snapshot)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}
