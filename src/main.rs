#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use niplock::app::NiplockApp;
#[cfg(not(target_arch = "wasm32"))]
use niplock::nostr_sync::signer_from_input;
#[cfg(not(target_arch = "wasm32"))]
use niplock::store::LocalStore;
#[cfg(not(target_arch = "wasm32"))]
use niplock::ui::SyncIndicator;
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::{Duration, sleep};
#[cfg(not(target_arch = "wasm32"))]
use tracing::{error, info};

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_RELAYS: [&str; 2] = ["wss://nip17.tomdwyer.uk", "wss://nip17.com"];

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("niplock=info".parse()?)
                .add_directive("nostr_relay_pool=warn".parse()?),
        )
        .with_target(false)
        .init();

    let signer_credential = std::env::var("NIPLOCK_SIGNER")
        .or_else(|_| std::env::var("NIPLOCK_NSEC"))
        .map_err(|_| {
            anyhow::anyhow!(
                "NIPLOCK_SIGNER or NIPLOCK_NSEC not set; provide nsec or bunker:// signer credential"
            )
        })?;
    let signer = signer_from_input(&signer_credential)?;
    let store = LocalStore::new()?;
    let indicator = Arc::new(SyncIndicator::default());

    let app = NiplockApp::new(
        signer,
        DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
        store,
        indicator.clone(),
    )
    .await?;

    let startup_app = app.clone();
    tokio::spawn(async move {
        startup_app.startup_sync().await;
    });

    info!("niplock running; press Ctrl+C to trigger graceful shutdown sync");

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

#[cfg(target_arch = "wasm32")]
fn main() {}
