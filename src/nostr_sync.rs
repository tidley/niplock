use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use nostr_connect::prelude::{NostrConnect, NostrConnectURI};
use nostr_sdk::prelude::*;
use tracing::{debug, warn};

#[cfg(target_arch = "wasm32")]
use nostr_browser_signer::BrowserSigner;

use crate::model::{PasswordEntry, PasswordEnvelope};

#[derive(Debug)]
pub struct SyncResult {
    pub downloaded: usize,
    pub uploaded: usize,
}

#[derive(Debug, Clone)]
pub struct NostrSync {
    client: Client,
    me: PublicKey,
    relays: Vec<String>,
}

impl NostrSync {
    pub async fn new(keys: Keys, relays: Vec<String>) -> Result<Self> {
        Self::new_with_signer(keys.into_nostr_signer(), relays).await
    }

    pub async fn new_with_signer(
        signer: Arc<dyn NostrSigner>,
        relays: Vec<String>,
    ) -> Result<Self> {
        let client = Client::new(signer);

        for relay in &relays {
            client.add_relay(relay).await?;
        }

        client.connect().await;
        client.wait_for_connection(Duration::from_secs(5)).await;
        let me = client.public_key().await?;

        Ok(Self { client, me, relays })
    }

    pub async fn sync(
        &self,
        local_entries: &HashMap<String, PasswordEntry>,
    ) -> Result<(HashMap<String, PasswordEntry>, SyncResult)> {
        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(self.me)
            .limit(2_000);

        let events = self
            .client
            .fetch_events(filter, Duration::from_secs(20))
            .await?;

        let mut remote_latest: HashMap<String, PasswordEntry> = HashMap::new();

        for event in events {
            let unwrapped = match self.client.unwrap_gift_wrap(&event).await {
                Ok(v) => v,
                Err(err) => {
                    debug!(error = %err, event_id = %event.id, "failed to unwrap gift wrap");
                    continue;
                }
            };

            let envelope: PasswordEnvelope =
                match serde_json::from_str::<PasswordEnvelope>(&unwrapped.rumor.content) {
                    Ok(v) if v.schema == "passwd.v1" => v,
                    Ok(_) => continue,
                    Err(_) => continue,
                };

            let mut entry = envelope.entry;
            entry.last_event_id = Some(event.id.to_hex());

            let merged = PasswordEntry::merge_prefer_newer(remote_latest.get(&entry.id), entry);
            remote_latest.insert(merged.id.clone(), merged);
        }

        let mut merged_entries = local_entries.clone();
        let mut downloaded = 0;

        for (id, remote_entry) in &remote_latest {
            let merged =
                PasswordEntry::merge_prefer_newer(merged_entries.get(id), remote_entry.clone());
            if merged_entries.get(id) != Some(&merged) {
                downloaded += 1;
            }
            merged_entries.insert(id.clone(), merged);
        }

        let mut uploaded = 0;

        for entry in merged_entries.values_mut() {
            let remote_entry = remote_latest.get(&entry.id);
            let should_upload = match remote_entry {
                None => true,
                Some(remote) => entry.updated_at > remote.updated_at,
            };

            if !should_upload {
                continue;
            }

            let payload = PasswordEnvelope::from_entry(entry.clone());
            let content = serde_json::to_string(&payload)?;

            let output = self
                .client
                .send_private_msg_to(
                    self.relays.clone(),
                    self.me,
                    content,
                    std::iter::empty::<Tag>(),
                )
                .await;

            match output {
                Ok(sent) => {
                    entry.last_event_id = Some(sent.val.to_hex());
                    uploaded += 1;
                }
                Err(err) => {
                    warn!(error = %err, entry_id = %entry.id, "failed to publish password entry");
                }
            }
        }

        Ok((
            merged_entries,
            SyncResult {
                downloaded,
                uploaded,
            },
        ))
    }

    pub async fn subscribe_live_updates(&self) -> Result<SubscriptionId> {
        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(self.me)
            .since(Timestamp::now());
        let output = self.client.subscribe(filter, None).await?;
        Ok(output.val)
    }

    pub async fn wait_for_live_update(&self, subscription_id: &SubscriptionId) -> Result<()> {
        let sub_id = subscription_id.clone();
        self.client
            .handle_notifications(move |notification| {
                let sub_id = sub_id.clone();
                async move {
                    if let RelayPoolNotification::Event {
                        subscription_id,
                        event,
                        ..
                    } = notification
                    {
                        if subscription_id == sub_id && event.kind == Kind::GiftWrap {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
            })
            .await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
}

pub fn signer_from_input(input: &str) -> Result<Arc<dyn NostrSigner>> {
    let credential = input.trim();

    if credential.is_empty() {
        anyhow::bail!("empty signer credential");
    }

    if credential.eq_ignore_ascii_case("nip07")
        || credential.eq_ignore_ascii_case("nos2xfox")
        || credential.eq_ignore_ascii_case("extension")
    {
        #[cfg(target_arch = "wasm32")]
        {
            let signer = BrowserSigner::new()?;
            return Ok(signer.into_nostr_signer());
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            anyhow::bail!("nip07 browser signer is only available in wasm/web builds");
        }
    }

    if let Some((uri_part, app_key)) = credential.split_once("::appkey=") {
        if uri_part.starts_with("nostrconnect://") {
            let uri = NostrConnectURI::parse(uri_part)?;
            let app_keys = Keys::parse(app_key)?;
            let signer = NostrConnect::new(uri, app_keys, Duration::from_secs(25), None)?;
            return Ok(signer.into_nostr_signer());
        }
    }

    if credential.starts_with("bunker://") || credential.starts_with("nostrconnect://") {
        let uri = NostrConnectURI::parse(credential)?;
        let session_keys = Keys::generate();
        let signer = NostrConnect::new(uri, session_keys, Duration::from_secs(25), None)?;
        return Ok(signer.into_nostr_signer());
    }

    let keys = Keys::parse(credential)?;
    Ok(keys.into_nostr_signer())
}
