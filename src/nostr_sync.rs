use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_utility::time;
use futures_util::future::join_all;
use nostr_connect::prelude::{NostrConnect, NostrConnectURI};
use nostr_sdk::nips::nip46::ResponseResult as NostrConnectResponseResult;
use nostr_sdk::prelude::*;
use tracing::{debug, warn};

#[cfg(target_arch = "wasm32")]
use nostr_browser_signer::BrowserSigner;

use crate::model::{PasswordEntry, PasswordEnvelope};

const MIN_RELAY_COPIES: usize = 2;
const PER_RELAY_FETCH_TIMEOUT_SECS: u64 = 5;
const RELAY_EVENT_FETCH_LIMIT: usize = 100;
const REMOTE_SIGNER_RELAY_EVENT_FETCH_LIMIT: usize = 24;
const SIGNER_OPERATION_TIMEOUT_SECS: u64 = 15;
const REMOTE_SIGNER_OPERATION_TIMEOUT_SECS: u64 = 5;
const MAX_REMOTE_UNWRAP_ATTEMPTS_PER_RELAY: usize = 8;
const NOSTR_CONNECT_TIMEOUT_SECS: u64 = 180;
const PREAPPROVED_NIP46_MARKER: &str = "::preapproved";

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
        self.sync_with_progress(local_entries, |_| {}).await
    }

    pub async fn sync_with_progress<F>(
        &self,
        local_entries: &HashMap<String, PasswordEntry>,
        mut progress: F,
    ) -> Result<(HashMap<String, PasswordEntry>, SyncResult)>
    where
        F: FnMut(String),
    {
        let mut remote_latest: HashMap<String, PasswordEntry> = HashMap::new();
        let mut remote_entries_by_relay: HashMap<String, Vec<(String, PasswordEntry)>> =
            HashMap::new();
        let mut entries_by_event_id: HashMap<EventId, PasswordEntry> = HashMap::new();
        let mut reachable_relays = Vec::<String>::new();
        let remote_signer = is_remote_signer_backend(&self.client).await;
        let fetch_limit = if remote_signer {
            REMOTE_SIGNER_RELAY_EVENT_FETCH_LIMIT
        } else {
            RELAY_EVENT_FETCH_LIMIT
        };
        let signer_operation_timeout_secs = if remote_signer {
            REMOTE_SIGNER_OPERATION_TIMEOUT_SECS
        } else {
            SIGNER_OPERATION_TIMEOUT_SECS
        };

        progress(format!(
            "Sync: reading {} local entries across {} relays",
            local_entries.len(),
            self.relays.len()
        ));
        if remote_signer {
            progress(format!(
                "Sync: remote signer mode enabled (limit {fetch_limit} events/relay, max {MAX_REMOTE_UNWRAP_ATTEMPTS_PER_RELAY} unwrap attempts/relay, {signer_operation_timeout_secs}s signer timeout)"
            ));
        }

        for relay in &self.relays {
            progress(format!(
                "Sync: queued fetch from {relay} ({}s timeout)",
                PER_RELAY_FETCH_TIMEOUT_SECS
            ));
        }
        progress("Sync: fetching all relays simultaneously".to_string());

        let fetches = self.relays.iter().cloned().map(|relay| {
            let client = self.client.clone();
            let me = self.me;
            async move {
                let filter = Filter::new()
                    .kind(Kind::GiftWrap)
                    .pubkey(me)
                    .limit(fetch_limit);
                let events = client
                    .fetch_events_from(
                        [relay.as_str()],
                        filter,
                        Duration::from_secs(PER_RELAY_FETCH_TIMEOUT_SECS),
                    )
                    .await;
                (relay, events)
            }
        });

        for (relay, events) in join_all(fetches).await {
            let events = match events {
                Ok(v) => {
                    progress(format!("Sync: {relay} returned {} events", v.len()));
                    reachable_relays.push(relay.clone());
                    v
                }
                Err(err) => {
                    progress(format!("Sync: {relay} fetch failed: {err}"));
                    warn!(error = %err, relay = %relay, "failed to fetch password entries from relay");
                    continue;
                }
            };

            let mut decoded = 0usize;
            let mut reused = 0usize;
            let total_events = events.len();
            let mut unwrap_attempts = 0usize;
            for event in events {
                if let Some(entry) = entries_by_event_id.get(&event.id) {
                    reused += 1;
                    remote_entries_by_relay
                        .entry(entry.id.clone())
                        .or_default()
                        .push((relay.clone(), entry.clone()));
                    continue;
                }
                if remote_signer && unwrap_attempts >= MAX_REMOTE_UNWRAP_ATTEMPTS_PER_RELAY {
                    progress(format!(
                        "Sync: {relay} remote unwrap attempt cap reached ({MAX_REMOTE_UNWRAP_ATTEMPTS_PER_RELAY})"
                    ));
                    break;
                }
                unwrap_attempts += 1;

                let position = decoded + reused + 1;
                if position == 1 || position % 10 == 0 {
                    progress(format!(
                        "Sync: {relay} unwrapping event {position}/{total_events}"
                    ));
                }
                let unwrapped = match time::timeout(
                    Some(Duration::from_secs(signer_operation_timeout_secs)),
                    self.client.unwrap_gift_wrap(&event),
                )
                .await
                {
                    Some(Ok(v)) => v,
                    Some(Err(err)) => {
                        debug!(error = %err, relay = %relay, event_id = %event.id, "failed to unwrap gift wrap");
                        continue;
                    }
                    None => {
                        progress(format!(
                            "Sync: {relay} decrypt timed out after {signer_operation_timeout_secs}s"
                        ));
                        debug!(relay = %relay, event_id = %event.id, "timed out unwrapping gift wrap");
                        continue;
                    }
                };

                let envelope: PasswordEnvelope =
                    match serde_json::from_str::<PasswordEnvelope>(&unwrapped.rumor.content) {
                        Ok(v) if v.schema == "niplock.v1" => v,
                        Ok(_) => continue,
                        Err(_) => continue,
                    };

                let mut entry = envelope.entry;
                entry.last_event_id = Some(event.id.to_hex());
                entries_by_event_id.insert(event.id, entry.clone());
                decoded += 1;
                remote_entries_by_relay
                    .entry(entry.id.clone())
                    .or_default()
                    .push((relay.clone(), entry.clone()));

                let merged = PasswordEntry::merge_prefer_newer(remote_latest.get(&entry.id), entry);
                remote_latest.insert(merged.id.clone(), merged);
            }
            progress(format!(
                "Sync: {relay} decoded {decoded} entries, reused {reused} duplicate events"
            ));
        }

        if reachable_relays.is_empty() && !self.relays.is_empty() {
            progress("Sync: no configured relay returned entries".to_string());
            anyhow::bail!("no configured relay returned password entries");
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
        let publish_relays = if reachable_relays.is_empty() {
            self.relays.clone()
        } else {
            reachable_relays.clone()
        };
        progress(format!(
            "Sync: {} reachable relays, {} remote entries, {} merged entries",
            reachable_relays.len(),
            remote_latest.len(),
            merged_entries.len()
        ));

        for entry in merged_entries.values_mut() {
            let remote_entry = remote_latest.get(&entry.id);
            let relay_copies = count_relay_copies(entry, remote_entries_by_relay.get(&entry.id));
            let should_upload = match remote_entry {
                None => true,
                Some(remote) => {
                    entry.updated_at > remote.updated_at || relay_copies < MIN_RELAY_COPIES
                }
            };

            if !should_upload {
                continue;
            }

            let target_relays = pick_target_relays(
                entry,
                remote_entries_by_relay.get(&entry.id),
                &publish_relays,
            );
            if target_relays.is_empty() {
                continue;
            }

            progress(format!(
                "Sync: publishing '{}' to {} relay(s); current copies {}",
                entry.service,
                target_relays.len(),
                relay_copies
            ));
            let payload = PasswordEnvelope::from_entry(entry.clone());
            let content = serde_json::to_string(&payload)?;

            let output = time::timeout(
                Some(Duration::from_secs(signer_operation_timeout_secs)),
                self.client.send_private_msg_to(
                    target_relays.clone(),
                    self.me,
                    content,
                    std::iter::empty::<Tag>(),
                ),
            )
            .await;

            match output {
                Some(Ok(sent)) => {
                    entry.last_event_id = Some(sent.val.to_hex());
                    uploaded += 1;
                    progress(format!(
                        "Sync: publish accepted by {} relay(s), failed {}",
                        sent.success.len(),
                        sent.failed.len()
                    ));
                    if sent.success.len() < MIN_RELAY_COPIES {
                        warn!(
                            entry_id = %entry.id,
                            target_relays = ?target_relays,
                            successful_relays = sent.success.len(),
                            required_relays = MIN_RELAY_COPIES,
                            failed_relays = ?sent.failed,
                            "password entry was not accepted by enough relays"
                        );
                    }
                }
                Some(Err(err)) => {
                    progress(format!(
                        "Sync: publish failed for '{}': {err}",
                        entry.service
                    ));
                    warn!(error = %err, entry_id = %entry.id, "failed to publish password entry");
                }
                None => {
                    progress(format!(
                        "Sync: publish timed out for '{}' after {signer_operation_timeout_secs}s",
                        entry.service
                    ));
                    warn!(entry_id = %entry.id, "timed out publishing password entry");
                }
            }
        }

        progress(format!(
            "Sync: complete, downloaded {downloaded}, uploaded {uploaded}"
        ));
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

async fn is_remote_signer_backend(client: &Client) -> bool {
    let Ok(signer) = client.signer().await else {
        return false;
    };
    match signer.backend() {
        SignerBackend::NostrConnect => true,
        SignerBackend::Custom(name) => name == "preapproved-nostr-connect",
        _ => false,
    }
}

fn count_relay_copies(
    entry: &PasswordEntry,
    remote: Option<&Vec<(String, PasswordEntry)>>,
) -> usize {
    let mut relays = HashSet::new();
    if let Some(remote) = remote {
        for (relay, remote_entry) in remote {
            if entry_payload_matches(entry, remote_entry) {
                relays.insert(relay);
            }
        }
    }
    relays.len()
}

fn pick_target_relays(
    entry: &PasswordEntry,
    remote: Option<&Vec<(String, PasswordEntry)>>,
    publish_relays: &[String],
) -> Vec<String> {
    let mut matching_relays = HashSet::new();
    if let Some(remote) = remote {
        for (relay, remote_entry) in remote {
            if entry_payload_matches(entry, remote_entry) {
                matching_relays.insert(relay.clone());
            }
        }
    }

    let needed = MIN_RELAY_COPIES
        .saturating_sub(matching_relays.len())
        .max(1);
    let mut targets = Vec::new();
    for relay in publish_relays {
        if matching_relays.contains(relay) {
            continue;
        }
        targets.push(relay.clone());
        if targets.len() >= needed {
            break;
        }
    }

    if targets.is_empty() {
        publish_relays.iter().take(1).cloned().collect()
    } else {
        targets
    }
}

fn entry_payload_matches(a: &PasswordEntry, b: &PasswordEntry) -> bool {
    a.id == b.id
        && a.service == b.service
        && a.username == b.username
        && a.secret == b.secret
        && a.notes == b.notes
        && a.updated_at == b.updated_at
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

    if let Some(base) = credential.strip_suffix(PREAPPROVED_NIP46_MARKER) {
        if let Some((uri_part, app_key)) = base.split_once("::appkey=") {
            if uri_part.starts_with("bunker://") {
                let uri = NostrConnectURI::parse(uri_part)?;
                let app_keys = Keys::parse(app_key)?;
                if let NostrConnectURI::Bunker {
                    remote_signer_public_key,
                    relays,
                    ..
                } = uri
                {
                    return Ok(Arc::new(PreapprovedNostrConnect::new(
                        app_keys,
                        remote_signer_public_key,
                        relays,
                        Duration::from_secs(NOSTR_CONNECT_TIMEOUT_SECS),
                    )));
                }
            }
        }
    }

    if let Some((uri_part, app_key)) = credential.split_once("::appkey=") {
        if uri_part.starts_with("nostrconnect://") || uri_part.starts_with("bunker://") {
            let uri = NostrConnectURI::parse(uri_part)?;
            let app_keys = Keys::parse(app_key)?;
            let signer = NostrConnect::new(
                uri,
                app_keys,
                Duration::from_secs(NOSTR_CONNECT_TIMEOUT_SECS),
                None,
            )?;
            return Ok(signer.into_nostr_signer());
        }
    }

    if credential.starts_with("bunker://") || credential.starts_with("nostrconnect://") {
        let uri = NostrConnectURI::parse(credential)?;
        let session_keys = Keys::generate();
        let signer = NostrConnect::new(
            uri,
            session_keys,
            Duration::from_secs(NOSTR_CONNECT_TIMEOUT_SECS),
            None,
        )?;
        return Ok(signer.into_nostr_signer());
    }

    let keys = Keys::parse(credential)?;
    Ok(keys.into_nostr_signer())
}

#[derive(Debug)]
struct PreapprovedNostrConnect {
    app_keys: Keys,
    remote_signer_public_key: PublicKey,
    relays: Vec<RelayUrl>,
    timeout: Duration,
    user_public_key: std::sync::Mutex<Option<PublicKey>>,
}

impl PreapprovedNostrConnect {
    fn new(
        app_keys: Keys,
        remote_signer_public_key: PublicKey,
        relays: Vec<RelayUrl>,
        timeout: Duration,
    ) -> Self {
        Self {
            app_keys,
            remote_signer_public_key,
            relays,
            timeout,
            user_public_key: std::sync::Mutex::new(None),
        }
    }

    async fn send_request(
        &self,
        req: NostrConnectRequest,
    ) -> Result<NostrConnectResponseResult, SignerError> {
        let client = Client::new(self.app_keys.clone());
        for relay in &self.relays {
            client
                .add_relay(relay.clone())
                .await
                .map_err(SignerError::backend)?;
        }
        client.connect().await;
        client.wait_for_connection(Duration::from_secs(5)).await;

        let filter = Filter::new()
            .kind(Kind::NostrConnect)
            .author(self.remote_signer_public_key)
            .pubkey(self.app_keys.public_key())
            .limit(0);
        let mut notifications = client.notifications();
        client
            .subscribe(filter, None)
            .await
            .map_err(SignerError::backend)?;

        let method = req.method();
        let message = NostrConnectMessage::request(&req);
        let request_id = message.id().to_string();
        let event =
            EventBuilder::nostr_connect(&self.app_keys, self.remote_signer_public_key, message)
                .and_then(|builder| builder.sign_with_keys(&self.app_keys))
                .map_err(SignerError::backend)?;
        client
            .send_event(&event)
            .await
            .map_err(SignerError::backend)?;

        let result = time::timeout(Some(self.timeout), async {
            while let Ok(notification) = notifications.recv().await {
                let RelayPoolNotification::Event { event, .. } = notification else {
                    continue;
                };
                if event.kind != Kind::NostrConnect || event.pubkey != self.remote_signer_public_key
                {
                    continue;
                }

                let decrypted = nip44::decrypt(
                    self.app_keys.secret_key(),
                    &event.pubkey,
                    event.content.as_str(),
                )
                .map_err(SignerError::backend)?;
                let message =
                    NostrConnectMessage::from_json(decrypted).map_err(SignerError::backend)?;

                if request_id != message.id() || !message.is_response() {
                    continue;
                }

                let response = message.to_response(method).map_err(SignerError::backend)?;
                if response.is_auth_url() {
                    return Err(SignerError::from(response.error.unwrap_or_else(|| {
                        "Amber requested additional authorization".to_string()
                    })));
                }
                if let Some(error) = response.error {
                    return Err(SignerError::from(error));
                }
                let Some(result) = response.result else {
                    return Err(SignerError::from("Amber returned an empty response"));
                };
                if result.is_error() {
                    return Err(SignerError::from("Amber returned an error"));
                }
                return Ok(result);
            }

            Err(SignerError::from("Amber response channel closed"))
        })
        .await
        .unwrap_or_else(|| Err(SignerError::from("Amber request timed out")));

        client.shutdown().await;
        result
    }
}

impl NostrSigner for PreapprovedNostrConnect {
    fn backend(&self) -> SignerBackend<'_> {
        SignerBackend::Custom(Cow::Borrowed("preapproved-nostr-connect"))
    }

    fn get_public_key(&self) -> BoxedFuture<'_, Result<PublicKey, SignerError>> {
        Box::pin(async move {
            if let Some(public_key) = *self
                .user_public_key
                .lock()
                .expect("public key cache poisoned")
            {
                return Ok(public_key);
            }

            let result = self.send_request(NostrConnectRequest::GetPublicKey).await?;
            let public_key = result.to_get_public_key().map_err(SignerError::backend)?;
            *self
                .user_public_key
                .lock()
                .expect("public key cache poisoned") = Some(public_key);
            Ok(public_key)
        })
    }

    fn sign_event(&self, unsigned: UnsignedEvent) -> BoxedFuture<'_, Result<Event, SignerError>> {
        Box::pin(async move {
            let result = self
                .send_request(NostrConnectRequest::SignEvent(unsigned))
                .await?;
            result.to_sign_event().map_err(SignerError::backend)
        })
    }

    fn nip04_encrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async move {
            let result = self
                .send_request(NostrConnectRequest::Nip04Encrypt {
                    public_key: *public_key,
                    text: content.to_string(),
                })
                .await?;
            result.to_nip04_encrypt().map_err(SignerError::backend)
        })
    }

    fn nip04_decrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        encrypted_content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async move {
            let result = self
                .send_request(NostrConnectRequest::Nip04Decrypt {
                    public_key: *public_key,
                    ciphertext: encrypted_content.to_string(),
                })
                .await?;
            result.to_nip04_decrypt().map_err(SignerError::backend)
        })
    }

    fn nip44_encrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        content: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async move {
            let result = self
                .send_request(NostrConnectRequest::Nip44Encrypt {
                    public_key: *public_key,
                    text: content.to_string(),
                })
                .await?;
            result.to_nip44_encrypt().map_err(SignerError::backend)
        })
    }

    fn nip44_decrypt<'a>(
        &'a self,
        public_key: &'a PublicKey,
        payload: &'a str,
    ) -> BoxedFuture<'a, Result<String, SignerError>> {
        Box::pin(async move {
            let result = self
                .send_request(NostrConnectRequest::Nip44Decrypt {
                    public_key: *public_key,
                    ciphertext: payload.to_string(),
                })
                .await?;
            result.to_nip44_decrypt().map_err(SignerError::backend)
        })
    }
}
