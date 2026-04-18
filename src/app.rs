use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use nostr_sdk::prelude::NostrSigner;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::nostr_sync::NostrSync;
use crate::store::{LocalStore, VaultSnapshot};
use crate::ui::SyncIndicator;

#[derive(Debug, Clone)]
pub struct NiplockApp {
    store: LocalStore,
    sync: NostrSync,
    indicator: Arc<SyncIndicator>,
    sync_lock: Arc<Mutex<()>>,
}

impl NiplockApp {
    pub async fn new(
        signer: Arc<dyn NostrSigner>,
        relays: Vec<String>,
        store: LocalStore,
        indicator: Arc<SyncIndicator>,
    ) -> Result<Self> {
        let sync = NostrSync::new_with_signer(signer, relays).await?;
        Ok(Self {
            store,
            sync,
            indicator,
            sync_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn startup_sync(&self) {
        self.indicator.set_syncing();

        if let Err(err) = self.perform_sync().await {
            error!(error = %err, "startup sync failed");
            self.indicator.set_error();
            return;
        }

        self.indicator.set_idle();
    }

    pub async fn shutdown_sync(&self) -> Result<()> {
        self.indicator.set_syncing();
        self.perform_sync().await?;
        self.sync.shutdown().await;
        self.indicator.set_idle();
        Ok(())
    }

    async fn perform_sync(&self) -> Result<()> {
        let _guard = self.sync_lock.lock().await;
        let mut snapshot: VaultSnapshot = self.store.load()?;
        let (entries, summary) = self.sync.sync(&snapshot.entries).await?;
        snapshot.entries = entries;
        snapshot.last_sync_at = Some(Utc::now());
        self.store.save(&snapshot)?;

        info!(
            downloaded = summary.downloaded,
            uploaded = summary.uploaded,
            "sync complete"
        );
        Ok(())
    }
}
