mod app;
mod model;
mod nostr_sync;
mod store;
mod ui;

use std::sync::Arc;

use anyhow::Result;
use app::PasswdApp;
use nostr_sdk::prelude::*;
use store::LocalStore;
use tokio::time::{Duration, sleep};
use tracing::{error, info};
use ui::SyncIndicator;

const DEFAULT_RELAYS: [&str; 2] = ["wss://nip17.tomdwyer.uk", "wss://nip17.com"];

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("passwd=info".parse()?)
                .add_directive("nostr_relay_pool=warn".parse()?),
        )
        .with_target(false)
        .init();

    let nsec = std::env::var("PASSWD_NSEC").map_err(|_| {
        anyhow::anyhow!("PASSWD_NSEC not set; provide your Nostr nsec for self-DM vault sync")
    })?;

    let keys = Keys::parse(&nsec)?;
    let store = LocalStore::new()?;
    let indicator = Arc::new(SyncIndicator::default());

    let app = PasswdApp::new(
        keys,
        DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
        store,
        indicator.clone(),
    )
    .await?;

    let startup_app = app.clone();
    tokio::spawn(async move {
        startup_app.startup_sync().await;
    });

    info!("passwd running; press Ctrl+C to trigger graceful shutdown sync");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                break;
            }
            _ = sleep(Duration::from_secs(5)) => {
                info!("status {}", indicator.render_hint());
            }
        }
    }

    if let Err(err) = app.shutdown_sync().await {
        error!(error = %err, "shutdown sync failed");
    }

    Ok(())
}
