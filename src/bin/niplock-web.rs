#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("niplock-web is a wasm binary. Build with: trunk serve --open");
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    use chrono::{DateTime, Utc};
    use gloo_events::EventListener;
    use gloo_timers::callback::Timeout;
    use gloo_timers::future::TimeoutFuture;
    use js_sys::{Date, Math};
    use niplock::model::PasswordEntry;
    use niplock::nostr_sync::{DEFAULT_RELAY_COPY_TARGET, NostrSync, signer_from_input};
    use nostr_sdk::JsonUtil;
    use nostr_sdk::prelude::{
        Client, EventBuilder, Filter, Keys, Kind, NostrConnectMessage, NostrConnectRequest,
        NostrConnectResponse, NostrConnectURI, RelayUrl, ToBech32, nip44,
    };
    use uuid::Uuid;
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{HtmlInputElement, HtmlTextAreaElement, WebSocket, window};
    use yew::prelude::*;

    const STORAGE_KEY: &str = "niplock.vault.v1";
    const ACTIVE_NPUB_STORAGE_KEY: &str = "niplock.active_npub.v1";
    const RELAYS_STORAGE_KEY: &str = "niplock.relays.v1";
    const RELAY_COPY_TARGET_STORAGE_KEY: &str = "niplock.relay_copy_target.v1";
    const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const MAX_SYNC_RELAYS: usize = 6;
    const AMBER_NIP46_PERMS: &str = "get_public_key,sign_event,nip44_encrypt,nip44_decrypt";
    const PREAPPROVED_NIP46_MARKER: &str = "::preapproved";
    const AMBER_APPROVAL_POLL_INTERVAL_MS: u32 = 2_000;
    const AMBER_APPROVAL_POLL_ATTEMPTS: u32 = 60;
    const RELAY_PROBE_COOLDOWN_MS: f64 = 30_000.0;
    const DEFAULT_RELAYS: [&str; MAX_SYNC_RELAYS] = [
        "wss://nip17.com",
        "wss://nip17.tomdwyer.uk",
        "wss://relay.momostr.pink",
        "wss://relay.bao.network",
        "wss://relay.cloistr.xyz",
        "wss://relay.paulstephenborile.com",
    ];

    const CSS: &str = r#"
:root {
  --bg: #0f1116;
  --panel: #171a21;
  --panel-2: #12151b;
  --line: #252a36;
  --text: #e8ecf3;
  --muted: #8e97a8;
  --teal: #35d0a1;
  --warn: #f0c35f;
  --err: #ef6e6e;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  overflow-x: hidden;
}
button { cursor: pointer; }
button:disabled { cursor: not-allowed; }
.app {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 240px 1fr;
}
.sidebar {
  background: #15181f;
  border-right: 1px solid var(--line);
  padding: 14px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.brand {
  padding: 6px 8px 16px 8px;
}
.brand h1 {
  margin: 0;
  font-family: "Space Grotesk", "Segoe UI", sans-serif;
  font-size: 1.35rem;
  letter-spacing: 0.03em;
}
.nav-item {
  width: 100%;
  text-align: left;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text);
  border-radius: 8px;
  padding: 10px;
  font-weight: 600;
  font-size: 0.92rem;
  cursor: pointer;
}
.nav-item:hover { background: #1d222d; }
.nav-item.active {
  background: #1d222d;
  border-color: #2d3443;
  box-shadow: inset 2px 0 0 0 var(--teal);
}
.side-spacer { flex: 1; }
.side-add {
  background: var(--teal);
  color: #08110f;
  border: 0;
  border-radius: 8px;
  padding: 10px;
  font-weight: 700;
}
.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.top {
  height: 58px;
  border-bottom: 1px solid var(--line);
  background: #171a21;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 0 16px;
}
.menu-btn {
  display: none;
  border: 1px solid #2d384b;
  background: #11151c;
  color: var(--text);
  border-radius: 6px;
  padding: 8px 10px;
  font-weight: 700;
  font-size: 1rem;
  line-height: 1;
}
.search {
  max-width: none;
  width: 100%;
  min-width: 0;
  background: #0f1218;
  border: 1px solid #262d3a;
  color: var(--text);
  border-radius: 6px;
  padding: 10px 12px;
}
.top-right {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-self: end;
}
.icon {
  color: var(--muted);
  font-size: 0.85rem;
}
.unlock {
  border: 1px solid #2d384b;
  background: #11151c;
  color: var(--teal);
  border-radius: 6px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  padding: 8px 12px;
  font-size: 0.75rem;
}
.unlock-panel {
  position: absolute;
  right: 16px;
  top: 62px;
  width: 340px;
  border: 1px solid var(--line);
  background: #171b22;
  border-radius: 8px;
  padding: 12px;
  z-index: 100;
}
.input, .textarea, .range {
  width: 100%;
  background: #10141b;
  color: var(--text);
  border: 1px solid #2a3240;
  border-radius: 6px;
  padding: 8px 10px;
}
.textarea {
  min-height: 70px;
  resize: vertical;
}
.page {
  padding: 16px;
  min-width: 0;
}
.section {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 8px;
  padding: 14px;
}
.muted { color: var(--muted); }
.row { display: flex; gap: 8px; align-items: center; }
.row > * { min-width: 0; }
.btn {
  border: 1px solid #2d384b;
  background: #1c222d;
  color: var(--text);
  border-radius: 6px;
  padding: 8px 10px;
  font-weight: 600;
}
.btn.primary {
  background: #1e2939;
  border-color: #355071;
  color: #dff4ff;
}
.btn.success {
  background: var(--teal);
  color: #08110f;
  border-color: var(--teal);
}
.btn.danger {
  color: #ffd7d7;
  border-color: #4f2d34;
  background: #26171a;
}
.explorer-head {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 12px;
  margin-bottom: 10px;
}
.stats { display: flex; gap: 10px; }
.stat {
  border: 1px solid var(--line);
  background: #1a1f28;
  padding: 10px;
  min-width: 140px;
}
.stat .k {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
}
.stat .v {
  margin-top: 4px;
  font-size: 1.7rem;
  font-weight: 700;
}
.tabs { display: flex; gap: 6px; margin-bottom: 10px; }
.tab {
  border: 1px solid #2a3240;
  background: #1a1f28;
  color: var(--muted);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
}
.tab.active {
  color: var(--text);
  border-color: #38635a;
  box-shadow: inset 0 -2px 0 0 var(--teal);
}
.table { width: 100%; border-collapse: collapse; table-layout: fixed; }
.table th, .table td {
  border-bottom: 1px solid var(--line);
  padding: 10px 8px;
  text-align: left;
  overflow-wrap: anywhere;
}
.table th {
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
}
.table tr:hover { background: #1b2029; }
.copy-cell { cursor: copy; }
.strength {
  width: 52px;
  height: 4px;
  background: #2a3240;
  border-radius: 999px;
  overflow: hidden;
}
.strength > i {
  display: block;
  height: 100%;
  background-color: var(--teal);
}
.vault-bottom {
  margin-top: 12px;
  display: grid;
  grid-template-columns: 1fr;
  gap: 12px;
}
.highlight {
  border: 1px solid var(--line);
  background: #151922;
  border-radius: 8px;
  padding: 16px;
}
.detail-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 270px;
  gap: 14px;
}
.detail-grid > * {
  min-width: 0;
}
.detail-main {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.detail-head {
  border: 1px solid var(--line);
  background: linear-gradient(140deg, #161b25, #141922);
  border-radius: 10px;
  padding: 14px;
}
.detail-title {
  font-size: 2rem;
  font-weight: 700;
  margin-bottom: 4px;
}
.detail-sub {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  color: var(--muted);
  font-size: 0.82rem;
}
.field-value {
  font-size: 1rem;
  font-weight: 600;
  overflow-wrap: anywhere;
}
.field-value.mono {
  font-family: "JetBrains Mono", monospace;
  letter-spacing: 0.02em;
}
.action-rail {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.danger-zone {
  border-color: #4f2d34;
  background: #24161a;
}
.detail-field {
  border: 1px solid var(--line);
  background: #141922;
  border-radius: 8px;
  padding: 10px;
  margin-bottom: 10px;
  min-width: 0;
}
.detail-label {
  font-size: 0.68rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--muted);
  margin-bottom: 4px;
}
.password-row {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 8px;
  align-items: center;
}
.sidebar-card {
  border: 1px solid var(--line);
  background: #171c25;
  border-radius: 8px;
  padding: 10px;
  margin-bottom: 10px;
  min-width: 0;
  overflow-wrap: anywhere;
}
.generator-grid {
  display: grid;
  grid-template-columns: 1fr 300px;
  gap: 12px;
}
.audit-grid, .settings-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.corner {
  position: fixed;
  top: 8px;
  right: 8px;
  width: 10px;
  height: 10px;
  border-radius: 999px;
  border: 1px solid #fff;
}
.corner.idle { background: #39d49e; }
.corner.syncing { background: #f0c35f; animation: pulse 1s infinite; }
.corner.error { background: #ef6e6e; }
.menu-overlay {
  display: none;
}
@keyframes pulse {
  0% { opacity: 0.45; transform: scale(0.85); }
  50% { opacity: 1; transform: scale(1.08); }
  100% { opacity: 0.45; transform: scale(0.85); }
}
@media (max-width: 1100px) {
  .app { grid-template-columns: 1fr; }
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: 270px;
    border-right: 1px solid var(--line);
    border-bottom: 0;
    z-index: 120;
    transform: translateX(-100%);
    transition: transform 180ms ease-out;
    overflow-y: auto;
  }
  .sidebar.mobile-open { transform: translateX(0); }
  .top { grid-template-columns: auto minmax(0, 1fr) auto; }
  .menu-btn { display: inline-block; }
  .menu-overlay {
    display: block;
    position: fixed;
    inset: 0;
    background: rgba(7, 10, 16, 0.55);
    z-index: 110;
  }
  .explorer-head { grid-template-columns: 1fr; }
  .vault-bottom, .detail-grid, .generator-grid { grid-template-columns: 1fr; }
  .audit-grid, .settings-grid { grid-template-columns: 1fr; }
  .strength-text { display: none; }
  .top { padding: 0 10px; gap: 8px; }
  .top-right { gap: 6px; }
  .unlock { padding: 8px 9px; }
}
@media (max-width: 700px) {
  .table-passwords .col-strength, .table-passwords .col-updated { display: none; }
  .table th, .table td { padding: 8px 6px; font-size: 0.9rem; }
  .table th { font-size: 0.62rem; }
  .top { min-height: 72px; height: auto; padding: 10px 10px; gap: 10px; }
  .top-right { gap: 8px; }
  .menu-btn { padding: 11px 13px; font-size: 1.08rem; }
  .search { padding: 11px 12px; font-size: 0.96rem; min-height: 44px; }
  .btn { padding: 10px 11px; font-size: 0.9rem; min-height: 44px; }
  .unlock { font-size: 0.8rem; padding: 10px 11px; min-height: 44px; }
  .page { padding: 12px; }
  .detail-sub { font-size: 0.75rem; gap: 8px; }
  .row { flex-wrap: wrap; }
  .password-row { grid-template-columns: 1fr auto; }
}
"#;

    #[derive(Clone, PartialEq)]
    enum SyncState {
        Idle,
        Syncing,
        Error(String),
    }

    #[derive(Clone, PartialEq)]
    enum Page {
        Vault,
        AddEntry,
        Generator,
        SecurityAudit,
        Settings,
    }

    #[derive(Clone, PartialEq)]
    enum UnlockMethod {
        Nsec,
        Amber,
        Nip07,
    }

    #[derive(Clone, Default, PartialEq)]
    struct Draft {
        id: Option<String>,
        service: String,
        username: String,
        secret: String,
        notes: String,
    }

    #[derive(Clone, PartialEq)]
    enum RelayProbeState {
        NotChecked,
        Checking,
        Reachable,
        Unreachable,
    }

    #[derive(Clone, PartialEq)]
    struct RelayProbe {
        relay: String,
        state: RelayProbeState,
        latency_ms: Option<u32>,
    }

    pub fn run() {
        yew::Renderer::<WebApp>::new().render();
    }

    #[function_component(WebApp)]
    fn web_app() -> Html {
        let entries = use_state(Vec::<PasswordEntry>::new);
        let page = use_state(|| Page::Vault);
        let selected_id = use_state(|| None::<String>);
        let search = use_state(String::new);

        let draft = use_state(Draft::default);
        let show_secret = use_state(|| false);
        let editor_open = use_state(|| false);
        let detail_secret_visible = use_state(|| false);

        let signer_credential = use_state(String::new);
        let active_npub = use_state(|| None::<String>);
        let unlock_input = use_state(String::new);
        let unlock_error = use_state(|| None::<String>);
        let unlock_panel_open = use_state(|| false);
        let unlock_method = use_state(|| UnlockMethod::Nsec);
        let amber_uri = use_state(|| None::<String>);
        let amber_session_credential = use_state(|| None::<String>);
        let amber_debug = use_state(Vec::<String>::new);
        let unlocked = use_state(|| false);
        let mobile_menu_open = use_state(|| false);

        let sync_state = use_state(|| SyncState::Idle);
        let sync_detail = use_state(|| None::<String>);
        let last_sync = use_state(|| None::<String>);
        let sync_in_flight = use_state(|| false);
        let relay_copies_by_entry = use_state(HashMap::<String, usize>::new);
        let live_sync = use_mut_ref(|| None::<NostrSync>);
        let live_subscription_id = use_mut_ref(|| None::<nostr_sdk::prelude::SubscriptionId>);
        let live_listener_running = use_mut_ref(|| false);
        let copy_notice = use_state(|| None::<String>);
        let relays = use_state(load_relays);
        let relay_probes = use_state({
            let relays = (*relays).clone();
            move || default_relay_probes(&relays)
        });
        let relay_copy_target = use_state(load_relay_copy_target);
        let last_relay_probe_at = use_state(|| None::<f64>);
        let relay_input = use_state(String::new);
        let relay_error = use_state(|| None::<String>);

        let gen_len = use_state(|| 18usize);
        let gen_upper = use_state(|| true);
        let gen_lower = use_state(|| true);
        let gen_numbers = use_state(|| true);
        let gen_symbols = use_state(|| true);
        let generated = use_state(|| generate_password(18, true, true, true, true));

        // Keep UI status consistent with actual in-flight sync activity.
        {
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let sync_in_flight = sync_in_flight.clone();
            use_effect_with(
                (*sync_in_flight, (*sync_state).clone()),
                move |(in_flight, state)| {
                    if !*in_flight && matches!(state, SyncState::Syncing) {
                        sync_state.set(SyncState::Idle);
                        sync_detail.set(None);
                    }
                    || ()
                },
            );
        }

        {
            let page = page.clone();
            let draft = draft.clone();
            let generated = generated.clone();
            let gen_len = gen_len.clone();
            let gen_upper = gen_upper.clone();
            let gen_lower = gen_lower.clone();
            let gen_numbers = gen_numbers.clone();
            let gen_symbols = gen_symbols.clone();
            use_effect_with(
                (
                    (*page).clone(),
                    *gen_len,
                    *gen_upper,
                    *gen_lower,
                    *gen_numbers,
                    *gen_symbols,
                ),
                move |_| {
                    let next_secret = generate_password(
                        *gen_len,
                        *gen_upper,
                        *gen_lower,
                        *gen_numbers,
                        *gen_symbols,
                    );
                    generated.set(next_secret.clone());
                    if *page == Page::AddEntry {
                        let mut next = (*draft).clone();
                        next.secret = next_secret;
                        draft.set(next);
                    }
                    || ()
                },
            );
        }

        {
            let entries = entries.clone();
            let signer_credential = signer_credential.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            let relays = relays.clone();
            use_effect_with((), move |_| {
                let doc_listener = window().and_then(|w| w.document()).map(|doc| {
                    let entries = entries.clone();
                    let signer_credential = signer_credential.clone();
                    let sync_state = sync_state.clone();
                    let sync_detail = sync_detail.clone();
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    let relay_copies_by_entry = relay_copies_by_entry.clone();
                    let relay_copy_target = relay_copy_target.clone();
                    let live_sync = live_sync.clone();
                    let unlocked = unlocked.clone();
                    let relays = relays.clone();
                    EventListener::new(&doc, "visibilitychange", move |_| {
                        if *unlocked {
                            if let Some(document) = window().and_then(|w| w.document()) {
                                if document.hidden() {
                                    spawn_sync(
                                        (*signer_credential).clone(),
                                        (*entries).clone(),
                                        (*relays).clone(),
                                        entries.clone(),
                                        sync_state.clone(),
                                        sync_detail.clone(),
                                        last_sync.clone(),
                                        sync_in_flight.clone(),
                                        relay_copies_by_entry.clone(),
                                        *relay_copy_target,
                                        live_sync.clone(),
                                    );
                                }
                            }
                        }
                    })
                });

                let pagehide_listener = window().map(|win| {
                    let entries = entries.clone();
                    let signer_credential = signer_credential.clone();
                    let sync_state = sync_state.clone();
                    let sync_detail = sync_detail.clone();
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    let relay_copies_by_entry = relay_copies_by_entry.clone();
                    let relay_copy_target = relay_copy_target.clone();
                    let live_sync = live_sync.clone();
                    let unlocked = unlocked.clone();
                    let relays = relays.clone();
                    EventListener::new(&win, "pagehide", move |_| {
                        if *unlocked {
                            spawn_sync(
                                (*signer_credential).clone(),
                                (*entries).clone(),
                                (*relays).clone(),
                                entries.clone(),
                                sync_state.clone(),
                                sync_detail.clone(),
                                last_sync.clone(),
                                sync_in_flight.clone(),
                                relay_copies_by_entry.clone(),
                                *relay_copy_target,
                                live_sync.clone(),
                            );
                        }
                    })
                });

                move || {
                    drop(doc_listener);
                    drop(pagehide_listener);
                }
            });
        }

        let start_live_listener = {
            let signer_credential = signer_credential.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            let unlocked = unlocked.clone();
            let relays = relays.clone();
            Callback::from(move |_| {
                if *live_listener_running.borrow() {
                    return;
                }
                *live_listener_running.borrow_mut() = true;

                let signer_credential = signer_credential.clone();
                let live_sync = live_sync.clone();
                let live_subscription_id = live_subscription_id.clone();
                let live_listener_running = live_listener_running.clone();
                let unlocked = unlocked.clone();
                let relays = (*relays).clone();
                spawn_local(async move {
                    loop {
                        if !*unlocked || !*live_listener_running.borrow() {
                            break;
                        }

                        let sync = if let Some(existing) = live_sync.borrow().clone() {
                            existing
                        } else {
                            let signer = match signer_from_input((*signer_credential).as_str()) {
                                Ok(v) => v,
                                Err(_) => break,
                            };

                            match NostrSync::new_with_signer(signer, relays.clone()).await {
                                Ok(v) => {
                                    *live_sync.borrow_mut() = Some(v.clone());
                                    v
                                }
                                Err(_) => break,
                            }
                        };

                        if live_subscription_id.borrow().is_none() {
                            match sync.subscribe_live_updates().await {
                                Ok(sub_id) => {
                                    *live_subscription_id.borrow_mut() = Some(sub_id);
                                }
                                Err(_) => break,
                            }
                        }

                        let Some(sub_id) = live_subscription_id.borrow().clone() else {
                            break;
                        };

                        if sync.wait_for_live_update(&sub_id).await.is_err() {
                            *live_sync.borrow_mut() = None;
                            *live_subscription_id.borrow_mut() = None;
                            break;
                        }

                        if !*unlocked || !*live_listener_running.borrow() {
                            break;
                        }

                        // Disabled automatic live-triggered sync to avoid perpetual sync loops
                        // when relays deliver our own recently published Gift Wrap events.
                    }

                    *live_listener_running.borrow_mut() = false;
                });
            })
        };

        let filtered_entries: Vec<PasswordEntry> = if *unlocked {
            let q = search.trim().to_ascii_lowercase();
            entries
                .iter()
                .filter(|entry| {
                    q.is_empty()
                        || entry.service.to_ascii_lowercase().contains(&q)
                        || entry.username.to_ascii_lowercase().contains(&q)
                        || entry
                            .notes
                            .as_ref()
                            .map(|v| v.to_ascii_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .cloned()
                .collect()
        } else {
            vec![]
        };

        let selected_entry = if *unlocked {
            selected_id
                .as_ref()
                .and_then(|id| entries.iter().find(|entry| &entry.id == id).cloned())
        } else {
            None
        };

        let on_nav_vault = {
            let page = page.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| {
                page.set(Page::Vault);
                mobile_menu_open.set(false);
            })
        };
        let on_nav_generator = {
            let page = page.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| {
                page.set(Page::Generator);
                mobile_menu_open.set(false);
            })
        };
        let on_nav_audit = {
            let page = page.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| {
                page.set(Page::SecurityAudit);
                mobile_menu_open.set(false);
            })
        };
        let on_nav_settings = {
            let page = page.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| {
                page.set(Page::Settings);
                mobile_menu_open.set(false);
            })
        };

        let on_add_item = {
            let page = page.clone();
            let editor_open = editor_open.clone();
            let draft = draft.clone();
            let selected_id = selected_id.clone();
            let show_secret = show_secret.clone();
            let unlocked = unlocked.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| {
                if !*unlocked {
                    return;
                }
                page.set(Page::AddEntry);
                selected_id.set(None);
                draft.set(Draft::default());
                show_secret.set(false);
                editor_open.set(true);
                mobile_menu_open.set(false);
            })
        };

        let on_toggle_mobile_menu = {
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| mobile_menu_open.set(!*mobile_menu_open))
        };
        let on_close_mobile_menu = {
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |_| mobile_menu_open.set(false))
        };

        let on_search = {
            let search = search.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                search.set(input.value());
            })
        };

        let on_toggle_unlock_panel = {
            let unlock_panel_open = unlock_panel_open.clone();
            let unlocked = unlocked.clone();
            let unlock_error = unlock_error.clone();
            let active_npub = active_npub.clone();
            let signer_credential = signer_credential.clone();
            let amber_uri = amber_uri.clone();
            let amber_session_credential = amber_session_credential.clone();
            let amber_debug = amber_debug.clone();
            let entries = entries.clone();
            let selected_id = selected_id.clone();
            let editor_open = editor_open.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            let relays = relays.clone();
            Callback::from(move |_| {
                if *unlocked {
                    let live_sync = live_sync.clone();
                    let sync_state = sync_state.clone();
                    let sync_detail_async = sync_detail.clone();
                    let signer_credential = (*signer_credential).clone();
                    let current_entries = (*entries).clone();
                    let relays = (*relays).clone();
                    let relay_copy_target = *relay_copy_target;
                    spawn_local(async move {
                        let mut sync = live_sync.borrow_mut().take();
                        if sync.is_none() {
                            let signer = match signer_from_input(&signer_credential) {
                                Ok(v) => v,
                                Err(_) => {
                                    sync_state.set(SyncState::Error(
                                        "lock sync: invalid signer".to_string(),
                                    ));
                                    sync_detail_async.set(None);
                                    return;
                                }
                            };
                            sync = match NostrSync::new_with_signer(signer, relays.clone()).await {
                                Ok(v) => Some(v),
                                Err(_) => {
                                    sync_state.set(SyncState::Error(
                                        "lock sync: relay connect failed".to_string(),
                                    ));
                                    sync_detail_async.set(None);
                                    return;
                                }
                            };
                        }

                        if let Some(sync) = sync {
                            let local = to_map(&current_entries);
                            match sync
                                .sync_with_progress_target(&local, relay_copy_target, |_| {})
                                .await
                            {
                                Ok((merged, _summary)) => {
                                    save_entries(&from_map(merged));
                                    sync_state.set(SyncState::Idle);
                                    sync_detail_async.set(None);
                                }
                                Err(_) => {
                                    sync_state
                                        .set(SyncState::Error("lock sync failed".to_string()));
                                    sync_detail_async.set(None);
                                }
                            }
                            sync.shutdown().await;
                        } else {
                            sync_state.set(SyncState::Idle);
                            sync_detail_async.set(None);
                        }
                    });

                    unlocked.set(false);
                    unlock_panel_open.set(false);
                    unlock_error.set(None);
                    active_npub.set(None);
                    amber_uri.set(None);
                    amber_session_credential.set(None);
                    amber_debug.set(Vec::new());
                    sync_detail.set(None);
                    relay_copies_by_entry.set(HashMap::new());
                    set_active_npub_storage(None);
                    entries.set(vec![]);
                    selected_id.set(None);
                    editor_open.set(false);
                    *live_listener_running.borrow_mut() = false;
                    *live_subscription_id.borrow_mut() = None;
                } else {
                    unlock_panel_open.set(!*unlock_panel_open);
                }
            })
        };

        let on_unlock_method_nsec = {
            let unlock_method = unlock_method.clone();
            let amber_debug = amber_debug.clone();
            Callback::from(move |_| {
                unlock_method.set(UnlockMethod::Nsec);
                amber_debug.set(Vec::new());
            })
        };
        let on_unlock_method_amber = {
            let unlock_method = unlock_method.clone();
            let amber_uri = amber_uri.clone();
            let amber_session_credential = amber_session_credential.clone();
            let amber_debug = amber_debug.clone();
            let unlock_error = unlock_error.clone();
            let relays = relays.clone();
            let signer_credential = signer_credential.clone();
            let active_npub = active_npub.clone();
            let unlocked = unlocked.clone();
            let unlock_panel_open = unlock_panel_open.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            Callback::from(move |_| {
                unlock_method.set(UnlockMethod::Amber);
                let (uri, credential) = if let (Some(uri), Some(credential)) =
                    (&*amber_uri, &*amber_session_credential)
                {
                    push_amber_debug(
                        amber_debug.clone(),
                        "Amber: reusing prepared NIP-46 session".to_string(),
                    );
                    (uri.clone(), credential.clone())
                } else {
                    match prepare_amber_session(&relays) {
                        Ok(v) => {
                            amber_uri.set(Some(v.0.clone()));
                            amber_session_credential.set(Some(v.1.clone()));
                            amber_debug.set(vec![
                                format!(
                                    "Amber: prepared NIP-46 session for {} relay(s)",
                                    relays.len()
                                ),
                                "Amber: URI includes secret and requested permissions".to_string(),
                            ]);
                            v
                        }
                        Err(err) => {
                            unlock_error
                                .set(Some(format!("Failed to create Amber session: {err}")));
                            return;
                        }
                    }
                };

                open_external_uri(&uri);
                push_amber_debug(
                    amber_debug.clone(),
                    "Amber: opened external URI".to_string(),
                );
                unlock_error.set(Some(
                    "Opened Amber. Approve niplock; sync will continue here.".to_string(),
                ));

                if !*sync_in_flight {
                    spawn_amber_unlock(
                        credential,
                        (*relays).clone(),
                        entries.clone(),
                        signer_credential.clone(),
                        active_npub.clone(),
                        unlocked.clone(),
                        unlock_error.clone(),
                        unlock_panel_open.clone(),
                        sync_state.clone(),
                        sync_detail.clone(),
                        last_sync.clone(),
                        sync_in_flight.clone(),
                        relay_copies_by_entry.clone(),
                        *relay_copy_target,
                        live_sync.clone(),
                        amber_debug.clone(),
                    );
                }
            })
        };
        let on_unlock_method_nip07 = {
            let unlock_method = unlock_method.clone();
            let amber_debug = amber_debug.clone();
            Callback::from(move |_| {
                unlock_method.set(UnlockMethod::Nip07);
                amber_debug.set(Vec::new());
            })
        };

        let on_unlock_input = {
            let unlock_input = unlock_input.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                unlock_input.set(input.value());
            })
        };

        let on_unlock_submit = {
            let unlock_input = unlock_input.clone();
            let unlock_method = unlock_method.clone();
            let signer_credential = signer_credential.clone();
            let active_npub = active_npub.clone();
            let amber_uri = amber_uri.clone();
            let amber_session_credential = amber_session_credential.clone();
            let amber_debug = amber_debug.clone();
            let unlocked = unlocked.clone();
            let unlock_error = unlock_error.clone();
            let unlock_panel_open = unlock_panel_open.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            let relays = relays.clone();
            Callback::from(move |_| {
                let credential = match &*unlock_method {
                    UnlockMethod::Nip07 => "nip07".to_string(),
                    UnlockMethod::Nsec => unlock_input.trim().to_string(),
                    UnlockMethod::Amber => {
                        if let Some(prepared) = &*amber_session_credential {
                            if *sync_in_flight {
                                push_amber_debug(
                                    amber_debug.clone(),
                                    "Amber: unlock already in progress".to_string(),
                                );
                                unlock_error.set(Some(
                                    "Waiting for Amber approval. Approve niplock in Amber."
                                        .to_string(),
                                ));
                                return;
                            }
                            spawn_amber_unlock(
                                prepared.clone(),
                                (*relays).clone(),
                                entries.clone(),
                                signer_credential.clone(),
                                active_npub.clone(),
                                unlocked.clone(),
                                unlock_error.clone(),
                                unlock_panel_open.clone(),
                                sync_state.clone(),
                                sync_detail.clone(),
                                last_sync.clone(),
                                sync_in_flight.clone(),
                                relay_copies_by_entry.clone(),
                                *relay_copy_target,
                                live_sync.clone(),
                                amber_debug.clone(),
                            );
                            return;
                        } else {
                            let (uri, prepared) = match prepare_amber_session(&relays) {
                                Ok(v) => v,
                                Err(err) => {
                                    unlock_error.set(Some(format!(
                                        "Failed to create Amber session: {err}"
                                    )));
                                    return;
                                }
                            };
                            amber_uri.set(Some(uri.clone()));
                            amber_session_credential.set(Some(prepared.clone()));
                            amber_debug.set(vec![
                                format!(
                                    "Amber: prepared NIP-46 session for {} relay(s)",
                                    relays.len()
                                ),
                                "Amber: URI includes secret and requested permissions".to_string(),
                            ]);
                            spawn_amber_unlock(
                                prepared,
                                (*relays).clone(),
                                entries.clone(),
                                signer_credential.clone(),
                                active_npub.clone(),
                                unlocked.clone(),
                                unlock_error.clone(),
                                unlock_panel_open.clone(),
                                sync_state.clone(),
                                sync_detail.clone(),
                                last_sync.clone(),
                                sync_in_flight.clone(),
                                relay_copies_by_entry.clone(),
                                *relay_copy_target,
                                live_sync.clone(),
                                amber_debug.clone(),
                            );
                            open_external_uri(&uri);
                            push_amber_debug(
                                amber_debug.clone(),
                                "Amber: opened external URI".to_string(),
                            );
                            unlock_error.set(Some(
                                "Opened Amber. Approve niplock to finish unlock.".to_string(),
                            ));
                            return;
                        }
                    }
                };

                match signer_from_input(&credential) {
                    Ok(signer) => {
                        let credential_for_sync = credential.clone();
                        let relays_for_sync = (*relays).clone();
                        let entries_state = entries.clone();
                        let signer_credential_state = signer_credential.clone();
                        let active_npub_state = active_npub.clone();
                        let unlocked_state = unlocked.clone();
                        let unlock_error_state = unlock_error.clone();
                        let unlock_panel_open_state = unlock_panel_open.clone();
                        let sync_state_state = sync_state.clone();
                        let sync_detail_state = sync_detail.clone();
                        let last_sync_state = last_sync.clone();
                        let sync_in_flight_state = sync_in_flight.clone();
                        let relay_copies_by_entry_state = relay_copies_by_entry.clone();
                        let relay_copy_target_state = *relay_copy_target;
                        let live_sync_state = live_sync.clone();
                        spawn_local(async move {
                            let next_npub = match signer.get_public_key().await {
                                Ok(pubkey) => pubkey.to_bech32().ok(),
                                Err(_) => None,
                            };
                            set_active_npub_storage(next_npub.as_deref());
                            active_npub_state.set(next_npub);

                            signer_credential_state.set(credential_for_sync.clone());
                            let cached_entries =
                                merge_entry_lists(&*entries_state, &load_entries());
                            entries_state.set(cached_entries.clone());
                            unlocked_state.set(true);
                            unlock_error_state.set(None);
                            unlock_panel_open_state.set(false);
                            spawn_sync(
                                credential_for_sync,
                                cached_entries,
                                relays_for_sync,
                                entries_state.clone(),
                                sync_state_state,
                                sync_detail_state,
                                last_sync_state,
                                sync_in_flight_state,
                                relay_copies_by_entry_state,
                                relay_copy_target_state,
                                live_sync_state,
                            );
                        });
                    }
                    Err(err) => {
                        unlock_error.set(Some(format!("Invalid signer credential: {err}")));
                    }
                }
            })
        };

        let on_sync_now = {
            let signer_credential = signer_credential.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            let relays = relays.clone();
            Callback::from(move |_| {
                if *unlocked {
                    spawn_sync(
                        (*signer_credential).clone(),
                        (*entries).clone(),
                        (*relays).clone(),
                        entries.clone(),
                        sync_state.clone(),
                        sync_detail.clone(),
                        last_sync.clone(),
                        sync_in_flight.clone(),
                        relay_copies_by_entry.clone(),
                        *relay_copy_target,
                        live_sync.clone(),
                    );
                }
            })
        };

        let on_probe_relays = {
            let relay_probes = relay_probes.clone();
            let relays = relays.clone();
            let last_relay_probe_at = last_relay_probe_at.clone();
            let relay_error = relay_error.clone();
            Callback::from(move |_| {
                let now = Date::now();
                if let Some(last) = *last_relay_probe_at {
                    if now - last < RELAY_PROBE_COOLDOWN_MS {
                        relay_error.set(Some("Relay health was checked recently".to_string()));
                        return;
                    }
                }
                last_relay_probe_at.set(Some(now));
                relay_error.set(None);
                probe_relays((*relays).clone(), relay_probes.clone());
            })
        };

        let on_entries_modified = {
            let signer_credential = signer_credential.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let sync_detail = sync_detail.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let relay_copies_by_entry = relay_copies_by_entry.clone();
            let relay_copy_target = relay_copy_target.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            let relays = relays.clone();
            Callback::from(move |current_entries: Vec<PasswordEntry>| {
                if *unlocked {
                    spawn_sync(
                        (*signer_credential).clone(),
                        current_entries,
                        (*relays).clone(),
                        entries.clone(),
                        sync_state.clone(),
                        sync_detail.clone(),
                        last_sync.clone(),
                        sync_in_flight.clone(),
                        relay_copies_by_entry.clone(),
                        *relay_copy_target,
                        live_sync.clone(),
                    );
                }
            })
        };

        let on_relay_input = {
            let relay_input = relay_input.clone();
            let relay_error = relay_error.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                relay_input.set(input.value());
                relay_error.set(None);
            })
        };

        let on_relay_copy_target = {
            let relay_copy_target = relay_copy_target.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                let parsed = input
                    .value()
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .map(sanitize_relay_copy_target)
                    .unwrap_or(DEFAULT_RELAY_COPY_TARGET);
                save_relay_copy_target(parsed);
                relay_copy_target.set(parsed);
            })
        };

        let on_add_relay = {
            let relays = relays.clone();
            let relay_input = relay_input.clone();
            let relay_error = relay_error.clone();
            let relay_probes = relay_probes.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            let unlocked = unlocked.clone();
            let start_live_listener = start_live_listener.clone();
            Callback::from(move |_| {
                let candidate = relay_input.trim().to_string();
                let normalized = match normalize_relay_url(&candidate) {
                    Some(v) => v,
                    None => {
                        relay_error
                            .set(Some("Enter a valid ws:// or wss:// relay URL".to_string()));
                        return;
                    }
                };

                let mut next = (*relays).clone();
                if next.len() >= MAX_SYNC_RELAYS {
                    relay_error.set(Some(format!(
                        "NIP-17 sync is limited to {MAX_SYNC_RELAYS} relays"
                    )));
                    return;
                }
                if next
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&normalized))
                {
                    relay_error.set(Some("Relay already added".to_string()));
                    return;
                }
                next.push(normalized);
                next = sanitize_relays(next);
                save_relays(&next);
                relays.set(next.clone());
                relay_probes.set(default_relay_probes(&next));
                *live_sync.borrow_mut() = None;
                *live_subscription_id.borrow_mut() = None;
                *live_listener_running.borrow_mut() = false;
                if *unlocked {
                    start_live_listener.emit(());
                }
                relay_input.set(String::new());
                relay_error.set(None);
            })
        };

        let on_remove_relay = {
            let relays = relays.clone();
            let relay_error = relay_error.clone();
            let relay_probes = relay_probes.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            let unlocked = unlocked.clone();
            let start_live_listener = start_live_listener.clone();
            Callback::from(move |relay: String| {
                if relays.len() <= 1 {
                    relay_error.set(Some("At least one relay is required".to_string()));
                    return;
                }
                let next: Vec<String> = relays
                    .iter()
                    .filter(|url| url.as_str() != relay.as_str())
                    .cloned()
                    .collect();
                if next.len() == relays.len() {
                    return;
                }
                save_relays(&next);
                relays.set(next.clone());
                relay_probes.set(default_relay_probes(&next));
                *live_sync.borrow_mut() = None;
                *live_subscription_id.borrow_mut() = None;
                *live_listener_running.borrow_mut() = false;
                if *unlocked {
                    start_live_listener.emit(());
                }
                relay_error.set(None);
            })
        };

        let on_draft_service = {
            let draft = draft.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                let mut next = (*draft).clone();
                next.service = input.value();
                draft.set(next);
            })
        };
        let on_draft_username = {
            let draft = draft.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                let mut next = (*draft).clone();
                next.username = input.value();
                draft.set(next);
            })
        };
        let on_draft_secret = {
            let draft = draft.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                let mut next = (*draft).clone();
                next.secret = input.value();
                draft.set(next);
            })
        };
        let on_draft_notes = {
            let draft = draft.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlTextAreaElement = e.target_unchecked_into();
                let mut next = (*draft).clone();
                next.notes = input.value();
                draft.set(next);
            })
        };

        let on_toggle_form_secret = {
            let show_secret = show_secret.clone();
            Callback::from(move |_| show_secret.set(!*show_secret))
        };

        let on_save_draft = {
            let entries = entries.clone();
            let draft = draft.clone();
            let editor_open = editor_open.clone();
            let page = page.clone();
            let selected_id = selected_id.clone();
            let on_entries_modified = on_entries_modified.clone();
            Callback::from(move |_| {
                if draft.service.trim().is_empty()
                    || draft.username.trim().is_empty()
                    || draft.secret.trim().is_empty()
                {
                    return;
                }

                let mut map = to_map(&entries);
                let id = draft
                    .id
                    .clone()
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                let last_event_id = map.get(&id).and_then(|v| v.last_event_id.clone());
                map.insert(
                    id.clone(),
                    PasswordEntry {
                        id: id.clone(),
                        service: draft.service.trim().to_string(),
                        username: draft.username.trim().to_string(),
                        secret: draft.secret.clone(),
                        notes: if draft.notes.trim().is_empty() {
                            None
                        } else {
                            Some(draft.notes.trim().to_string())
                        },
                        updated_at: Utc::now(),
                        last_event_id,
                    },
                );

                let next = from_map(map);
                save_entries(&next);
                entries.set(next.clone());
                if *page == Page::AddEntry {
                    selected_id.set(None);
                } else {
                    selected_id.set(Some(id));
                }
                editor_open.set(false);
                page.set(Page::Vault);
                draft.set(Draft::default());
                on_entries_modified.emit(next);
            })
        };

        let on_cancel_draft = {
            let editor_open = editor_open.clone();
            let draft = draft.clone();
            let page = page.clone();
            Callback::from(move |_| {
                editor_open.set(false);
                page.set(Page::Vault);
                draft.set(Draft::default());
            })
        };

        let on_toggle_detail_secret = {
            let detail_secret_visible = detail_secret_visible.clone();
            Callback::from(move |_| detail_secret_visible.set(!*detail_secret_visible))
        };

        let on_gen_len = {
            let gen_len = gen_len.clone();
            Callback::from(move |e: InputEvent| {
                let input: HtmlInputElement = e.target_unchecked_into();
                if let Ok(v) = input.value().parse::<usize>() {
                    gen_len.set(v.clamp(8, 128));
                }
            })
        };
        let on_gen_upper = {
            let gen_upper = gen_upper.clone();
            Callback::from(move |e: Event| {
                let input: HtmlInputElement = e.target_unchecked_into();
                gen_upper.set(input.checked());
            })
        };
        let on_gen_lower = {
            let gen_lower = gen_lower.clone();
            Callback::from(move |e: Event| {
                let input: HtmlInputElement = e.target_unchecked_into();
                gen_lower.set(input.checked());
            })
        };
        let on_gen_numbers = {
            let gen_numbers = gen_numbers.clone();
            Callback::from(move |e: Event| {
                let input: HtmlInputElement = e.target_unchecked_into();
                gen_numbers.set(input.checked());
            })
        };
        let on_gen_symbols = {
            let gen_symbols = gen_symbols.clone();
            Callback::from(move |e: Event| {
                let input: HtmlInputElement = e.target_unchecked_into();
                gen_symbols.set(input.checked());
            })
        };

        let on_generate = {
            let generated = generated.clone();
            let gen_len = gen_len.clone();
            let gen_upper = gen_upper.clone();
            let gen_lower = gen_lower.clone();
            let gen_numbers = gen_numbers.clone();
            let gen_symbols = gen_symbols.clone();
            Callback::from(move |_| {
                generated.set(generate_password(
                    *gen_len,
                    *gen_upper,
                    *gen_lower,
                    *gen_numbers,
                    *gen_symbols,
                ));
            })
        };

        let on_generate_and_fill = {
            let draft = draft.clone();
            let generated = generated.clone();
            let gen_len = gen_len.clone();
            let gen_upper = gen_upper.clone();
            let gen_lower = gen_lower.clone();
            let gen_numbers = gen_numbers.clone();
            let gen_symbols = gen_symbols.clone();
            Callback::from(move |_| {
                let next_secret =
                    generate_password(*gen_len, *gen_upper, *gen_lower, *gen_numbers, *gen_symbols);
                generated.set(next_secret.clone());
                let mut next = (*draft).clone();
                next.secret = next_secret;
                draft.set(next);
            })
        };

        let on_copy_generated = {
            let generated = generated.clone();
            let copy_notice = copy_notice.clone();
            Callback::from(move |_| {
                copy_to_clipboard(
                    (*generated).clone(),
                    copy_notice.clone(),
                    "Generated secret copied".to_string(),
                );
            })
        };

        let on_use_generated = {
            let page = page.clone();
            let selected_id = selected_id.clone();
            let draft = draft.clone();
            let show_secret = show_secret.clone();
            let editor_open = editor_open.clone();
            let generated = generated.clone();
            let unlocked = unlocked.clone();
            Callback::from(move |_| {
                if !*unlocked {
                    return;
                }
                page.set(Page::AddEntry);
                selected_id.set(None);
                let mut next = (*draft).clone();
                next.secret = (*generated).clone();
                draft.set(next);
                show_secret.set(false);
                editor_open.set(true);
            })
        };

        let corner_class = match &*sync_state {
            SyncState::Idle => "corner idle",
            SyncState::Syncing => "corner syncing",
            SyncState::Error(_) => "corner error",
        };

        let sync_label = match &*sync_state {
            SyncState::Idle => "Idle".to_string(),
            SyncState::Syncing => format_sync_label("Syncing", (*sync_detail).clone()),
            SyncState::Error(err) => format!("Error: {err}"),
        };

        let weak_count = entries
            .iter()
            .filter(|e| entropy_bits(&e.secret) < 60.0)
            .count();
        let health_score = if entries.is_empty() {
            100.0
        } else {
            ((entries.len() - weak_count) as f64 / entries.len() as f64) * 100.0
        };
        let on_open_entry_from_audit = {
            let page = page.clone();
            let selected_id = selected_id.clone();
            let draft = draft.clone();
            let editor_open = editor_open.clone();
            let show_secret = show_secret.clone();
            let mobile_menu_open = mobile_menu_open.clone();
            Callback::from(move |entry: PasswordEntry| {
                page.set(Page::Vault);
                selected_id.set(Some(entry.id.clone()));
                draft.set(Draft {
                    id: Some(entry.id),
                    service: entry.service,
                    username: entry.username,
                    secret: entry.secret,
                    notes: entry.notes.unwrap_or_default(),
                });
                show_secret.set(false);
                editor_open.set(true);
                mobile_menu_open.set(false);
            })
        };

        html! {
            <>
                <style>{CSS}</style>
                <div class={corner_class}></div>
                <div class="app">
                    <aside class={classes!("sidebar", if *mobile_menu_open { Some("mobile-open") } else { None })}>
                        <div class="brand">
                            <h1 onclick={on_nav_vault.clone()} style="cursor:pointer;">{"niplock"}</h1>
                        </div>
                        <button class={classes!("nav-item", if *page == Page::Vault { Some("active") } else { None })} onclick={on_nav_vault}>{"Passwords"}</button>
                        <button class={classes!("nav-item", if *page == Page::Generator { Some("active") } else { None })} onclick={on_nav_generator}>{"Generator"}</button>
                        <button class={classes!("nav-item", if *page == Page::SecurityAudit { Some("active") } else { None })} onclick={on_nav_audit}>{"Security Audit"}</button>
                        <button class={classes!("nav-item", if *page == Page::Settings { Some("active") } else { None })} onclick={on_nav_settings}>{"Settings"}</button>
                        <div class="side-spacer"></div>
                        <button class="side-add" onclick={on_add_item.clone()}>{"+ Add Item"}</button>
                    </aside>
                    if *mobile_menu_open {
                        <div class="menu-overlay" onclick={on_close_mobile_menu}></div>
                    }

                    <section class="main">
                        <header class="top">
                            <button class="menu-btn" onclick={on_toggle_mobile_menu}>{"☰"}</button>
                            <input class="search" placeholder="Search passwords..." value={(*search).clone()} oninput={on_search} />
                            <div class="top-right">
                                <button class="btn" onclick={on_sync_now.clone()} disabled={!*unlocked}>{"Sync"}</button>
                                <button class="unlock" onclick={on_toggle_unlock_panel}>{ if *unlocked { "Lock" } else { "Unlock" } }</button>
                            </div>
                            if *unlock_panel_open {
                                <div class="unlock-panel">
                                    <div style="font-weight:700; margin-bottom:8px;">{"Unlock Vault Sync"}</div>
                                    <div class="row" style="margin-bottom:8px;">
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Nsec { Some("primary") } else { None })} onclick={on_unlock_method_nsec}>{"NSEC"}</button>
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Amber { Some("primary") } else { None })} onclick={on_unlock_method_amber}>{"Amber"}</button>
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Nip07 { Some("primary") } else { None })} onclick={on_unlock_method_nip07}>{"nos2xfox"}</button>
                                    </div>
                                    if *unlock_method == UnlockMethod::Nip07 {
                                        <div class="muted" style="margin-top: 8px; font-size: 0.85rem;">{"Using browser signer via NIP-07 (nos2xfox)."}</div>
                                    } else {
                                        if *unlock_method == UnlockMethod::Nsec {
                                            <input
                                                class="input"
                                                type="password"
                                                placeholder="nsec1..."
                                                value={(*unlock_input).clone()}
                                                oninput={on_unlock_input.clone()}
                                            />
                                        } else if amber_uri.is_some() {
                                            <div class="muted" style="margin-top: 8px; font-size: 0.78rem; overflow-wrap:anywhere; font-family:monospace; line-height:1.35;">
                                                <div style="font-weight:700; color:var(--text); margin-bottom:4px;">{"Amber debug"}</div>
                                                if amber_debug.is_empty() {
                                                    <div>{"Amber: session prepared"}</div>
                                                } else {
                                                    {for amber_debug.iter().map(|line| html! {
                                                        <div>{line.clone()}</div>
                                                    })}
                                                }
                                            </div>
                                        } else {
                                            <div class="muted" style="margin-top: 8px; font-size: 0.8rem;">
                                                {"Tap Amber to open Amber and approve niplock."}
                                            </div>
                                        }
                                    }
                                    if let Some(err) = &*unlock_error {
                                        <div style="color: var(--err); margin-top: 8px; font-size: 0.85rem;">{err.clone()}</div>
                                    }
                                    <div class="row" style="margin-top: 10px; justify-content: flex-end;">
                                        <button class="btn" onclick={on_unlock_submit}>{"Unlock + Sync"}</button>
                                    </div>
                                </div>
                            }
                        </header>

                        <div class="page">
                            if let Some(msg) = &*copy_notice {
                                <div class="muted" style="margin-bottom:10px;">{msg.clone()}</div>
                            }

                            {
                                match &*page {
                                    Page::Vault => render_vault_page(
                                        &filtered_entries,
                                        &selected_entry,
                                        *editor_open,
                                        *unlocked,
                                        &draft,
                                        *show_secret,
                                        *detail_secret_visible,
                                        health_score,
                                        last_sync.as_ref().cloned(),
                                        weak_count,
                                        (*generated).clone(),
                                        *gen_len,
                                        *gen_upper,
                                        *gen_lower,
                                        *gen_numbers,
                                        *gen_symbols,
                                        on_gen_len.clone(),
                                        on_gen_upper.clone(),
                                        on_gen_lower.clone(),
                                        on_gen_numbers.clone(),
                                        on_gen_symbols.clone(),
                                        on_generate.clone(),
                                        on_use_generated.clone(),
                                        on_generate_and_fill.clone(),
                                        on_draft_service,
                                        on_draft_username,
                                        on_draft_secret,
                                        on_draft_notes,
                                        on_toggle_form_secret,
                                        on_save_draft,
                                        on_cancel_draft,
                                        on_toggle_detail_secret,
                                        selected_id.clone(),
                                        draft.clone(),
                                        editor_open.clone(),
                                        show_secret.clone(),
                                        entries.clone(),
                                        (*relay_copies_by_entry).clone(),
                                        *relay_copy_target,
                                        copy_notice.clone(),
                                        on_entries_modified.clone(),
                                    ),
                                    Page::AddEntry => render_add_entry_page(
                                        &draft,
                                        *show_secret,
                                        *gen_len,
                                        *gen_upper,
                                        *gen_lower,
                                        *gen_numbers,
                                        *gen_symbols,
                                        on_gen_len,
                                        on_gen_upper,
                                        on_gen_lower,
                                        on_gen_numbers,
                                        on_gen_symbols,
                                        on_draft_service,
                                        on_draft_username,
                                        on_draft_secret,
                                        on_draft_notes,
                                        on_toggle_form_secret,
                                        on_generate_and_fill,
                                        on_save_draft,
                                        on_cancel_draft,
                                    ),
                                    Page::Generator => render_generator_page(
                                        (*generated).clone(),
                                        *gen_len,
                                        *gen_upper,
                                        *gen_lower,
                                        *gen_numbers,
                                        *gen_symbols,
                                        on_gen_len,
                                        on_gen_upper,
                                        on_gen_lower,
                                        on_gen_numbers,
                                        on_gen_symbols,
                                        on_generate,
                                        on_copy_generated,
                                    ),
                                    Page::SecurityAudit => render_audit_page(
                                        &entries,
                                        weak_count,
                                        health_score,
                                        (*relay_copies_by_entry).clone(),
                                        *relay_copy_target,
                                        on_open_entry_from_audit.clone(),
                                    ),
                                    Page::Settings => render_settings_page(
                                        (*signer_credential).clone(),
                                        (*active_npub).clone(),
                                        *unlocked,
                                        sync_label,
                                        (*sync_detail).clone(),
                                        last_sync.as_ref().cloned(),
                                        (*relays).clone(),
                                        *relay_copy_target,
                                        (*relay_input).clone(),
                                        (*relay_error).clone(),
                                        (*relay_probes).clone(),
                                        on_relay_input.clone(),
                                        on_relay_copy_target.clone(),
                                        on_add_relay.clone(),
                                        on_remove_relay.clone(),
                                        on_probe_relays.clone(),
                                    ),
                                }
                            }
                        </div>
                    </section>
                </div>
            </>
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_vault_page(
        filtered_entries: &[PasswordEntry],
        selected_entry: &Option<PasswordEntry>,
        editor_open: bool,
        unlocked: bool,
        draft: &UseStateHandle<Draft>,
        show_secret: bool,
        detail_secret_visible: bool,
        _health_score: f64,
        _last_sync: Option<String>,
        _weak_count: usize,
        generated: String,
        gen_len: usize,
        gen_upper: bool,
        gen_lower: bool,
        gen_numbers: bool,
        gen_symbols: bool,
        on_gen_len: Callback<InputEvent>,
        on_gen_upper: Callback<Event>,
        on_gen_lower: Callback<Event>,
        on_gen_numbers: Callback<Event>,
        on_gen_symbols: Callback<Event>,
        on_generate: Callback<MouseEvent>,
        on_use_generated: Callback<MouseEvent>,
        on_generate_and_fill: Callback<MouseEvent>,
        on_draft_service: Callback<InputEvent>,
        on_draft_username: Callback<InputEvent>,
        on_draft_secret: Callback<InputEvent>,
        on_draft_notes: Callback<InputEvent>,
        on_toggle_form_secret: Callback<MouseEvent>,
        on_save_draft: Callback<MouseEvent>,
        on_cancel_draft: Callback<MouseEvent>,
        on_toggle_detail_secret: Callback<MouseEvent>,
        selected_id: UseStateHandle<Option<String>>,
        draft_state: UseStateHandle<Draft>,
        editor_open_state: UseStateHandle<bool>,
        show_secret_state: UseStateHandle<bool>,
        entries_state: UseStateHandle<Vec<PasswordEntry>>,
        relay_copies_by_entry: HashMap<String, usize>,
        relay_copy_target: usize,
        copy_notice: UseStateHandle<Option<String>>,
        on_entries_modified: Callback<Vec<PasswordEntry>>,
    ) -> Html {
        if let Some(entry) = selected_entry {
            let on_back = {
                let selected_id = selected_id.clone();
                Callback::from(move |_| selected_id.set(None))
            };

            let edit_entry = entry.clone();
            let on_edit = {
                let draft_state = draft_state.clone();
                let editor_open_state = editor_open_state.clone();
                let show_secret_state = show_secret_state.clone();
                Callback::from(move |_| {
                    draft_state.set(Draft {
                        id: Some(edit_entry.id.clone()),
                        service: edit_entry.service.clone(),
                        username: edit_entry.username.clone(),
                        secret: edit_entry.secret.clone(),
                        notes: edit_entry.notes.clone().unwrap_or_default(),
                    });
                    show_secret_state.set(false);
                    editor_open_state.set(true);
                })
            };

            let delete_id = entry.id.clone();
            let on_purge = {
                let entries_state = entries_state.clone();
                let selected_id = selected_id.clone();
                let on_entries_modified = on_entries_modified.clone();
                Callback::from(move |_| {
                    let mut map = to_map(&entries_state);
                    map.remove(&delete_id);
                    let next = from_map(map);
                    save_entries(&next);
                    entries_state.set(next.clone());
                    selected_id.set(None);
                    on_entries_modified.emit(next);
                })
            };

            let user_to_copy = entry.username.clone();
            let service_for_user_copy = entry.service.clone();
            let on_copy_user = {
                let copy_notice = copy_notice.clone();
                Callback::from(move |_| {
                    copy_to_clipboard(
                        user_to_copy.clone(),
                        copy_notice.clone(),
                        format!("Copied username for {service_for_user_copy}"),
                    );
                })
            };

            let secret_to_copy = entry.secret.clone();
            let service_for_secret_copy = entry.service.clone();
            let on_copy_secret = {
                let copy_notice = copy_notice.clone();
                Callback::from(move |_| {
                    copy_to_clipboard(
                        secret_to_copy.clone(),
                        copy_notice.clone(),
                        format!("Copied password for {service_for_secret_copy}"),
                    );
                })
            };
            let bits = entropy_bits(&entry.secret);

            html! {
                <>
                    <div class="row muted" style="margin-bottom: 10px;">
                        <button class="btn" onclick={on_back}>{"← Back to Passwords"}</button>
                        <span>{format!("Passwords / {}", entry.service)}</span>
                    </div>
                    <div class="detail-grid">
                        <div class="detail-main">
                            <div class="detail-head">
                                <div class="detail-label">{"Credential"}</div>
                                <div class="detail-title">{entry.service.clone()}</div>
                                <div class="detail-sub">
                                    <span>{format!("Last modified {}", entry.updated_at.format("%b %d, %Y"))}</span>
                                    <span>{format!("{bits:.1} bits entropy")}</span>
                                    <span>{strength_label(bits)}</span>
                                </div>
                            </div>

                            <div class="detail-field">
                                <div class="detail-label">{"Username / Email"}</div>
                                <div class="row" style="justify-content: space-between;">
                                    <div ondblclick={on_copy_user.clone()} class="copy-cell field-value">{entry.username.clone()}</div>
                                    <button class="btn" onclick={on_copy_user}>{"Copy"}</button>
                                </div>
                            </div>

                            <div class="detail-field">
                                <div class="detail-label">{"Primary Key"}</div>
                                <div class="password-row">
                                    <div ondblclick={on_copy_secret.clone()} class={classes!("copy-cell", "field-value", "mono")}>
                                        {if detail_secret_visible { entry.secret.clone() } else { "••••••••••••••••".to_string() }}
                                    </div>
                                    <button class="btn" onclick={on_toggle_detail_secret.clone()}>{if detail_secret_visible { "Hide" } else { "Reveal" }}</button>
                                    <button class="btn" onclick={on_copy_secret}>{"Copy"}</button>
                                </div>
                                <div class="row" style="margin-top:8px;">
                                    <span class="strength"><i style={format!("width:{}%;", strength_width_pct(bits))}></i></span>
                                    <span class="muted strength-text">{format!("{} security", strength_label(bits))}</span>
                                </div>
                            </div>

                            <div class="detail-field">
                                <div class="detail-label">{"Website"}</div>
                                <div class="field-value mono">{format!("https://{}.com", entry.service.to_ascii_lowercase().replace(' ', ""))}</div>
                            </div>

                            <div class="detail-field">
                                <div class="detail-label">{"Notes"}</div>
                                <div class="muted">{entry.notes.clone().unwrap_or_else(|| "No notes for this credential.".to_string())}</div>
                            </div>

                            if editor_open {
                                <div class="section" style="margin-top:10px;">
                                    <div style="font-weight:700; margin-bottom: 8px;">{"Edit Entry"}</div>
                                    <input class="input" placeholder="Title" value={draft.service.clone()} oninput={on_draft_service}/>
                                    <input class="input" placeholder="Username" value={draft.username.clone()} oninput={on_draft_username}/>
                                    <div class="row">
                                        <input class="input" type={if show_secret { "text" } else { "password" }} placeholder="Password" value={draft.secret.clone()} oninput={on_draft_secret}/>
                                        <button class="btn" onclick={on_toggle_form_secret.clone()}>{"👁"}</button>
                                        <button class="btn" onclick={on_generate_and_fill.clone()}>{"Generate"}</button>
                                    </div>
                                    <textarea class="textarea" placeholder="Security notes" value={draft.notes.clone()} oninput={on_draft_notes}></textarea>
                                    <div class="row" style="justify-content:flex-end; margin-top: 8px;">
                                        <button class="btn" onclick={on_cancel_draft}>{"Cancel"}</button>
                                        <button class="btn success" onclick={on_save_draft}>{"Save"}</button>
                                    </div>
                                </div>
                            }
                        </div>

                        <aside class="action-rail">
                            <div class="sidebar-card">
                                <div class="detail-label">{"Actions"}</div>
                                <button class="btn success" style="width:100%;" onclick={on_edit}>{"Edit Entry"}</button>
                            </div>
                            <div class="sidebar-card">
                                <div class="detail-label">{"Metadata"}</div>
                                <div class="muted">{format!("Last modified: {}", entry.updated_at.format("%b %d, %Y %H:%M UTC"))}</div>
                                if let Some(copies) = relay_copies_by_entry.get(&entry.id) {
                                    <div class="muted" style="margin-top:6px;">{format!("Relay copies: {copies}/{relay_copy_target} target")}</div>
                                }
                                <div class="muted" style="margin-top:6px; font-family:monospace; overflow-wrap:anywhere; word-break:break-word;">{format!("Record id: {}", entry.id)}</div>
                                if let Some(event_id) = &entry.last_event_id {
                                    <div class="muted" style="margin-top:6px; font-family:monospace; overflow-wrap:anywhere; word-break:break-word;">{format!("Last sync event: {}", event_id)}</div>
                                }
                            </div>
                            <div class="sidebar-card danger-zone">
                                <div class="detail-label" style="color:#ff9aa4;">{"Danger Zone"}</div>
                                <div class="muted">{"This action permanently deletes the credential from your local vault."}</div>
                                <button class="btn danger" style="width:100%; margin-top:10px;" onclick={on_purge}>{"Permanently Delete"}</button>
                            </div>
                        </aside>
                    </div>
                </>
            }
        } else {
            let generated_bits = entropy_bits(&generated);
            html! {
                <>
                    <div class="section">
                        if !unlocked {
                            <div class="muted" style="margin-bottom:10px;">{"Unlock to load your passwords."}</div>
                        }
                        <table class="table table-passwords">
                            <thead>
                                <tr>
                                    <th>{"Title"}</th>
                                    <th>{"Username"}</th>
                                    <th class="col-strength">{"Strength"}</th>
                                    <th class="col-relay">{"Relay Copies"}</th>
                                    <th class="col-updated">{"Last Modified"}</th>
                                </tr>
                            </thead>
                            <tbody>
                            {for filtered_entries.iter().map(|entry| {
                                let entry_for_select = entry.clone();
                                let selected_id = selected_id.clone();
                                let on_select = Callback::from(move |_| {
                                    selected_id.set(Some(entry_for_select.id.clone()));
                                });

                                let username_value = entry.username.clone();
                                let service_for_username = entry.service.clone();
                                let on_copy_user = {
                                    let copy_notice = copy_notice.clone();
                                    Callback::from(move |_| {
                                        copy_to_clipboard(
                                            username_value.clone(),
                                            copy_notice.clone(),
                                            format!("Copied username for {service_for_username}"),
                                        );
                                    })
                                };

                                let secret_value = entry.secret.clone();
                                let service_for_secret = entry.service.clone();
                                let on_copy_secret = {
                                    let copy_notice = copy_notice.clone();
                                    Callback::from(move |_| {
                                        copy_to_clipboard(
                                            secret_value.clone(),
                                            copy_notice.clone(),
                                            format!("Copied password for {service_for_secret}"),
                                        );
                                    })
                                };

                                let bits = entropy_bits(&entry.secret);
                                html! {
                                    <tr onclick={on_select}>
                                        <td><strong>{entry.service.clone()}</strong></td>
                                        <td class="copy-cell" ondblclick={on_copy_user}>{entry.username.clone()}</td>
                                        <td class="col-strength">
                                            <div class="row">
                                                <span class="strength"><i style={format!("width:{}%;", strength_width_pct(bits))}></i></span>
                                                <span class="muted strength-text">{strength_label(bits)}</span>
                                            </div>
                                        </td>
                                        <td class="col-relay">{
                                            relay_copies_by_entry
                                                .get(&entry.id)
                                                .map(|copies| format!("{copies}/{relay_copy_target}"))
                                                .unwrap_or_else(|| "?".to_string())
                                        }</td>
                                        <td class={classes!("copy-cell", "col-updated")} ondblclick={on_copy_secret}>{entry.updated_at.format("%b %d, %Y").to_string()}</td>
                                    </tr>
                                }
                            })}
                            </tbody>
                        </table>
                    </div>

                    <div class="vault-bottom">
                        <div class="highlight">
                            <h3 style="margin-top:0;">{"Password Generator"}</h3>
                            <div class="muted">{"Create high-entropy randomized keys instantly."}</div>
                            <div style="margin-top:10px;">
                                <div class="detail-label">{"Length"}</div>
                                <div class="row">
                                    <input class="range" type="range" min="8" max="128" value={gen_len.to_string()} oninput={on_gen_len}/>
                                    <strong>{gen_len}</strong>
                                </div>
                                <div class="row" style="flex-wrap:wrap; margin-top:8px;">
                                    <label class="row"><input type="checkbox" checked={gen_upper} onchange={on_gen_upper}/><span>{"A-Z"}</span></label>
                                    <label class="row"><input type="checkbox" checked={gen_lower} onchange={on_gen_lower}/><span>{"a-z"}</span></label>
                                    <label class="row"><input type="checkbox" checked={gen_numbers} onchange={on_gen_numbers}/><span>{"0-9"}</span></label>
                                    <label class="row"><input type="checkbox" checked={gen_symbols} onchange={on_gen_symbols}/><span>{"!@#"}</span></label>
                                </div>
                            </div>
                            <div style="margin-top:12px; font-weight:700; font-family:'JetBrains Mono', monospace; overflow-wrap:anywhere;">{generated.clone()}</div>
                            <div class="row" style="margin-top:8px;">
                                <span class="strength"><i style={format!("width:{}%;", strength_width_pct(generated_bits))}></i></span>
                                <span class="muted strength-text">{format!("{generated_bits:.1} bits ({})", strength_label(generated_bits))}</span>
                            </div>
                            <div class="row" style="margin-top:10px;">
                                <button class="btn" onclick={on_generate.clone()}>{"Regenerate"}</button>
                                <button class="btn success" onclick={on_use_generated} disabled={!unlocked}>{"Use in Add Entry"}</button>
                            </div>
                        </div>
                    </div>
                </>
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_add_entry_page(
        draft: &UseStateHandle<Draft>,
        show_secret: bool,
        gen_len: usize,
        gen_upper: bool,
        gen_lower: bool,
        gen_numbers: bool,
        gen_symbols: bool,
        on_gen_len: Callback<InputEvent>,
        on_gen_upper: Callback<Event>,
        on_gen_lower: Callback<Event>,
        on_gen_numbers: Callback<Event>,
        on_gen_symbols: Callback<Event>,
        on_draft_service: Callback<InputEvent>,
        on_draft_username: Callback<InputEvent>,
        on_draft_secret: Callback<InputEvent>,
        on_draft_notes: Callback<InputEvent>,
        on_toggle_form_secret: Callback<MouseEvent>,
        on_generate_and_fill: Callback<MouseEvent>,
        on_save_draft: Callback<MouseEvent>,
        on_cancel_draft: Callback<MouseEvent>,
    ) -> Html {
        let draft_bits = entropy_bits(&draft.secret);
        html! {
            <>
                <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"Add Entry"}</h2>
                <div class="section" style="margin-top:12px; max-width: 760px;">
                    <input class="input" placeholder="Title" value={draft.service.clone()} oninput={on_draft_service}/>
                    <input class="input" placeholder="Username" value={draft.username.clone()} oninput={on_draft_username}/>
                    <div class="row">
                        <input class="input" type={if show_secret { "text" } else { "password" }} placeholder="Password" value={draft.secret.clone()} oninput={on_draft_secret}/>
                        <button class="btn" onclick={on_toggle_form_secret}>{"👁"}</button>
                        <button class="btn" onclick={on_generate_and_fill}>{"Generate"}</button>
                    </div>
                    <div class="detail-label" style="margin-top:10px;">{"Password Generator Controls"}</div>
                    <div class="row">
                        <input class="range" type="range" min="8" max="128" value={gen_len.to_string()} oninput={on_gen_len}/>
                        <strong>{gen_len}</strong>
                    </div>
                    <div class="row" style="flex-wrap:wrap; margin-top:8px;">
                        <label class="row"><input type="checkbox" checked={gen_upper} onchange={on_gen_upper}/><span>{"A-Z"}</span></label>
                        <label class="row"><input type="checkbox" checked={gen_lower} onchange={on_gen_lower}/><span>{"a-z"}</span></label>
                        <label class="row"><input type="checkbox" checked={gen_numbers} onchange={on_gen_numbers}/><span>{"0-9"}</span></label>
                        <label class="row"><input type="checkbox" checked={gen_symbols} onchange={on_gen_symbols}/><span>{"!@#"}</span></label>
                    </div>
                    <div class="row" style="margin-top:8px;">
                        <span class="strength"><i style={format!("width:{}%;", strength_width_pct(draft_bits))}></i></span>
                        <span class="muted">{format!("Entropy: {draft_bits:.1} bits ({})", strength_label(draft_bits))}</span>
                    </div>
                    <textarea class="textarea" placeholder="Notes" value={draft.notes.clone()} oninput={on_draft_notes}></textarea>
                    <div class="row" style="justify-content:flex-end; margin-top: 8px;">
                        <button class="btn" onclick={on_cancel_draft}>{"Cancel"}</button>
                        <button class="btn success" onclick={on_save_draft}>{"Save"}</button>
                    </div>
                </div>
            </>
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_generator_page(
        generated: String,
        gen_len: usize,
        gen_upper: bool,
        gen_lower: bool,
        gen_numbers: bool,
        gen_symbols: bool,
        on_gen_len: Callback<InputEvent>,
        on_gen_upper: Callback<Event>,
        on_gen_lower: Callback<Event>,
        on_gen_numbers: Callback<Event>,
        on_gen_symbols: Callback<Event>,
        on_generate: Callback<MouseEvent>,
        on_copy_generated: Callback<MouseEvent>,
    ) -> Html {
        let bits = entropy_bits(&generated);
        html! {
            <>
                <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"Generator"}</h2>

                <div class="section" style="margin-bottom:12px;">
                    <div class="detail-label">{"Generated Secret"}</div>
                    <div style="font-size:1rem; font-family:'JetBrains Mono', monospace; overflow-wrap:anywhere; word-break:break-word; white-space:normal;">
                        {generated.clone()}
                    </div>
                    <div class="row" style="margin-top:10px;">
                        <button class="btn" onclick={on_copy_generated.clone()}>{"Copy"}</button>
                        <button class="btn" onclick={on_generate.clone()}>{"Regenerate"}</button>
                    </div>
                </div>

                <div class="generator-grid">
                    <div class="section">
                        <div class="detail-label">{"Character Length"}</div>
                        <div style="font-size: 2.6rem; font-weight:700; color: var(--teal);">{gen_len}</div>
                        <input class="range" type="range" min="8" max="128" value={gen_len.to_string()} oninput={on_gen_len} />

                        <div style="margin-top: 16px;" class="detail-label">{"Inclusion Parameters"}</div>
                        <div class="row" style="margin:8px 0;"><input type="checkbox" checked={gen_upper} onchange={on_gen_upper}/><span>{"Uppercase"}</span></div>
                        <div class="row" style="margin:8px 0;"><input type="checkbox" checked={gen_lower} onchange={on_gen_lower}/><span>{"Lowercase"}</span></div>
                        <div class="row" style="margin:8px 0;"><input type="checkbox" checked={gen_numbers} onchange={on_gen_numbers}/><span>{"Numbers"}</span></div>
                        <div class="row" style="margin:8px 0;"><input type="checkbox" checked={gen_symbols} onchange={on_gen_symbols}/><span>{"Symbols"}</span></div>
                    </div>

                    <div class="section">
                        <div class="detail-label">{"Entropy Rating"}</div>
                        <div style="font-size:2.2rem; font-weight:700;">{format!("{bits:.1} Bits")}</div>
                        <div class="muted" style="margin-top: 8px;">{format!("{}", strength_label(bits))}</div>
                        <div style="margin-top: 18px;" class="row">
                            <button class="btn success" onclick={on_generate}>{"Generate"}</button>
                            <button class="btn" onclick={on_copy_generated}>{"Copy"}</button>
                        </div>
                    </div>
                </div>
            </>
        }
    }

    fn render_audit_page(
        entries: &[PasswordEntry],
        weak_count: usize,
        health_score: f64,
        relay_copies_by_entry: HashMap<String, usize>,
        relay_copy_target: usize,
        on_open_entry: Callback<PasswordEntry>,
    ) -> Html {
        let mut ranked = entries.to_vec();
        ranked.sort_by(|a, b| {
            entropy_bits(&a.secret)
                .partial_cmp(&entropy_bits(&b.secret))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        html! {
            <>
                <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"Security Audit"}</h2>
                <div class="muted" style="margin:6px 0 12px 0;">{"Cryptographic posture and rotation readiness"}</div>

                <div class="audit-grid">
                    <div class="section">
                        <div class="detail-label">{"Vault Health"}</div>
                        <div style="font-size:2.4rem; color:var(--teal); font-weight:700;">{format!("{health_score:.1}%")}</div>
                    </div>
                    <div class="section">
                        <div class="detail-label">{"Weak Credentials"}</div>
                        <div style="font-size:2.4rem; color:var(--warn); font-weight:700;">{weak_count}</div>
                    </div>
                </div>

                <div class="section" style="margin-top:12px;">
                    <div class="detail-label">{"Prioritized Rotation Queue"}</div>
                    <table class="table">
                        <thead>
                            <tr>
                                <th>{"Title"}</th>
                                <th>{"Username"}</th>
                                <th>{"Entropy"}</th>
                                <th>{"Relay Copies"}</th>
                            </tr>
                        </thead>
                        <tbody>
                        {for ranked.iter().take(12).map(|entry| {
                            let bits = entropy_bits(&entry.secret);
                            let entry_for_open = entry.clone();
                            let on_open_entry = on_open_entry.clone();
                            let on_select = Callback::from(move |_| on_open_entry.emit(entry_for_open.clone()));
                            html! {
                                <tr onclick={on_select}>
                                    <td>{entry.service.clone()}</td>
                                    <td>{entry.username.clone()}</td>
                                    <td>{format!("{bits:.1} bits")}</td>
                                    <td>{
                                        relay_copies_by_entry
                                            .get(&entry.id)
                                            .map(|copies| format!("{copies}/{relay_copy_target}"))
                                            .unwrap_or_else(|| "?".to_string())
                                    }</td>
                                </tr>
                            }
                        })}
                        </tbody>
                    </table>
                </div>
            </>
        }
    }

    fn render_settings_page(
        signer_credential: String,
        active_npub: Option<String>,
        unlocked: bool,
        sync_label: String,
        sync_detail: Option<String>,
        last_sync: Option<String>,
        relays: Vec<String>,
        relay_copy_target: usize,
        relay_input: String,
        relay_error: Option<String>,
        relay_probes: Vec<RelayProbe>,
        on_relay_input: Callback<InputEvent>,
        on_relay_copy_target: Callback<InputEvent>,
        on_add_relay: Callback<MouseEvent>,
        on_remove_relay: Callback<String>,
        on_probe_relays: Callback<MouseEvent>,
    ) -> Html {
        let total_relays = relay_probes.len();
        let reachable_relays = relay_probes
            .iter()
            .filter(|probe| probe.state == RelayProbeState::Reachable)
            .count();
        let checking_relays = relay_probes
            .iter()
            .filter(|probe| probe.state == RelayProbeState::Checking)
            .count();
        let relay_health_score = if total_relays == 0 {
            0.0
        } else {
            (reachable_relays as f64 / total_relays as f64) * 100.0
        };
        let avg_latency_ms = {
            let samples: Vec<u32> = relay_probes
                .iter()
                .filter_map(|probe| {
                    if probe.state == RelayProbeState::Reachable {
                        probe.latency_ms
                    } else {
                        None
                    }
                })
                .collect();
            if samples.is_empty() {
                None
            } else {
                let total: u32 = samples.iter().sum();
                Some(total / samples.len() as u32)
            }
        };

        html! {
            <>
                <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"System Preferences"}</h2>
                <div class="muted" style="margin-top:4px;">{format!("Version: {APP_VERSION}")}</div>

                <div class="section">
                    <div class="detail-label">{"Sync State"}</div>
                    <div style="font-size:1.25rem; font-weight:700; color:var(--teal);">{if unlocked { "Unlocked" } else { "Locked" }}</div>
                    <div class="muted">{sync_label}</div>
                    if let Some(detail) = sync_detail {
                        <div class="muted" style="margin-top:6px; font-family:monospace; font-size:0.82rem; overflow-wrap:anywhere;">{detail}</div>
                    }
                </div>

                <div class="section" style="margin-top:12px;">
                    <div class="detail-label">{"Nostr Credentials"}</div>
                    <div class="muted">{"Loaded in session only"}</div>
                    <div style="margin-top:8px;">
                        <div class="detail-label">{"Current npub"}</div>
                        <div style="font-family: monospace; overflow-wrap:anywhere;">
                            {active_npub.unwrap_or_else(|| "(not available)".to_string())}
                        </div>
                    </div>
                    <div style="margin-top:8px; font-family: monospace;">
                        {if signer_credential.is_empty() { "(not loaded)".to_string() } else { "signer••••••••••••••••".to_string() }}
                    </div>
                    if let Some(ts) = last_sync {
                        <div class="muted" style="margin-top:6px; font-size:0.76rem; opacity:0.85;">{format!("Last sync: {}", format_human_timestamp(&ts))}</div>
                    }
                </div>

                <div class="section" style="margin-top:12px;">
                    <div class="row" style="justify-content:space-between;">
                        <div>
                            <div class="detail-label">{"Relay Mesh"}</div>
                            <div class="muted">{format!("{reachable_relays}/{total_relays} reachable, max {MAX_SYNC_RELAYS}")}</div>
                        </div>
                        <button class="btn" onclick={on_probe_relays} disabled={!unlocked}>{"Recheck Relays"}</button>
                    </div>
                    <div class="row" style="margin-top:10px;">
                        <span class="strength"><i style={format!("width:{relay_health_score:.1}%;")}></i></span>
                        <span class="muted">{format!("Health: {relay_health_score:.1}%")}</span>
                    </div>
                    <div class="muted" style="margin-top:8px;">
                        {if checking_relays > 0 {
                            format!("{checking_relays} relay checks in progress")
                        } else if let Some(avg) = avg_latency_ms {
                            format!("Average handshake latency: {avg} ms")
                        } else {
                            "Relay health is checked only when you press Recheck Relays.".to_string()
                        }}
                    </div>
                    <div style="margin-top:10px;" class="detail-label">{"Relay Copy Target"}</div>
                    <div class="muted" style="margin-top:4px;">{"Minimum relays that should hold each password entry."}</div>
                    <div class="row" style="margin-top:6px;">
                        <input
                            class="input"
                            type="number"
                            min="1"
                            max={MAX_SYNC_RELAYS.to_string()}
                            value={relay_copy_target.to_string()}
                            oninput={on_relay_copy_target}
                        />
                    </div>
                    <div style="margin-top:10px;" class="detail-label">{"Configured Relays"}</div>
                    <div class="muted" style="margin-top:4px;">{format!("NIP-17 DM sync uses up to {MAX_SYNC_RELAYS} relays.")}</div>
                    <div class="row" style="margin-top:6px;">
                        <input
                            class="input"
                            placeholder="wss://relay.example.com"
                            value={relay_input}
                            oninput={on_relay_input}
                        />
                        <button class="btn" onclick={on_add_relay} disabled={!unlocked}>{"Add"}</button>
                    </div>
                    if let Some(err) = relay_error {
                        <div style="margin-top:6px; color:var(--err); font-size:0.85rem;">{err}</div>
                    }
                    <div style="margin-top:8px;">
                        {for relays.iter().map(|relay| {
                            let relay_to_remove = relay.clone();
                            let on_remove_relay = on_remove_relay.clone();
                            let on_remove = Callback::from(move |_| on_remove_relay.emit(relay_to_remove.clone()));
                            let probe = relay_probes.iter().find(|probe| probe.relay == *relay);
                            let (status_color, status_text) = match probe.map(|probe| (&probe.state, probe.latency_ms)) {
                                Some((RelayProbeState::Reachable, Some(latency))) => ("var(--teal)", format!("Reachable · {latency} ms")),
                                Some((RelayProbeState::Reachable, None)) => ("var(--teal)", "Reachable".to_string()),
                                Some((RelayProbeState::Unreachable, _)) => ("var(--err)", "Unreachable".to_string()),
                                Some((RelayProbeState::Checking, _)) => ("var(--warn)", "Checking...".to_string()),
                                _ => ("var(--muted)", "Not checked".to_string()),
                            };
                            html! {
                                <div class="row" style="justify-content:space-between; margin-top:6px; border:1px solid var(--line); border-radius:6px; padding:8px 10px; gap:10px;">
                                    <div style="font-family:monospace; font-size:0.85rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1;">{relay.clone()}</div>
                                    <div style={format!("color:{status_color}; font-weight:600; font-size:0.82rem; white-space:nowrap;")}>{status_text}</div>
                                    <button class="btn danger" onclick={on_remove} disabled={!unlocked}>{"Remove"}</button>
                                </div>
                            }
                        })}
                    </div>
                </div>

                <div class="section" style="margin-top:12px; border-color:#49242a; background:#25171b;">
                    <div class="detail-label" style="color:#ff9aa4;">{"Terminal Action: Wipe Vault"}</div>
                    <div class="muted">{"Irreversible deletion of local credentials and cryptographic keys."}</div>
                    <button class="btn danger" style="margin-top:10px;">{"Initiate Purge"}</button>
                </div>
            </>
        }
    }

    fn strength_label(bits: f64) -> &'static str {
        if bits >= 110.0 {
            "Very Strong"
        } else if bits >= 85.0 {
            "Strong"
        } else if bits >= 60.0 {
            "Moderate"
        } else {
            "Weak"
        }
    }

    fn format_sync_label(base: &str, detail: Option<String>) -> String {
        if let Some(detail) = detail {
            if detail.is_empty() {
                base.to_string()
            } else {
                format!("{base}: {detail}")
            }
        } else {
            base.to_string()
        }
    }

    fn strength_width_pct(bits: f64) -> f64 {
        let pct = (bits / 1.2).clamp(0.0, 100.0);
        if pct == 0.0 { 0.0 } else { pct.max(4.0) }
    }

    fn entropy_bits(secret: &str) -> f64 {
        if secret.is_empty() {
            return 0.0;
        }
        let chars: Vec<char> = secret.chars().collect();
        let len = chars.len() as f64;
        if len == 0.0 {
            return 0.0;
        }

        let unique: HashSet<char> = chars.iter().copied().collect();
        unique
            .iter()
            .map(|ch| {
                let count = chars.iter().filter(|c| **c == *ch).count() as f64;
                let p = count / len;
                -p * p.log2()
            })
            .sum::<f64>()
            * len
    }

    fn format_human_timestamp(ts: &str) -> String {
        match DateTime::parse_from_rfc3339(ts) {
            Ok(parsed) => parsed
                .with_timezone(&Utc)
                .format("%b %d, %Y %H:%M UTC")
                .to_string(),
            Err(_) => ts.to_string(),
        }
    }

    fn generate_password(
        len: usize,
        uppercase: bool,
        lowercase: bool,
        numbers: bool,
        symbols: bool,
    ) -> String {
        let mut alphabet = String::new();
        if uppercase {
            alphabet.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        }
        if lowercase {
            alphabet.push_str("abcdefghijklmnopqrstuvwxyz");
        }
        if numbers {
            alphabet.push_str("0123456789");
        }
        if symbols {
            alphabet.push_str("!@#$%^&*()_+-=[]{};:,.<>?/");
        }
        if alphabet.is_empty() {
            alphabet.push_str("abcdefghijklmnopqrstuvwxyz");
        }

        let chars: Vec<char> = alphabet.chars().collect();
        (0..len)
            .map(|_| {
                let idx = (Math::random() * chars.len() as f64) as usize;
                chars[idx.min(chars.len() - 1)]
            })
            .collect()
    }

    fn default_relay_strings() -> Vec<String> {
        DEFAULT_RELAYS
            .iter()
            .map(|relay| relay.to_string())
            .collect()
    }

    fn relay_urls_from_strings(relays: &[String]) -> Vec<RelayUrl> {
        relays
            .iter()
            .take(MAX_SYNC_RELAYS)
            .filter_map(|relay| RelayUrl::parse(relay).ok())
            .collect()
    }

    fn default_relay_probes(relays: &[String]) -> Vec<RelayProbe> {
        relays
            .iter()
            .map(|relay| RelayProbe {
                relay: relay.to_string(),
                state: RelayProbeState::NotChecked,
                latency_ms: None,
            })
            .collect()
    }

    fn probe_relays(relays: Vec<String>, relay_probes: UseStateHandle<Vec<RelayProbe>>) {
        let relays = sanitize_relays(relays);
        relay_probes.set(
            relays
                .iter()
                .map(|relay| RelayProbe {
                    relay: relay.to_string(),
                    state: RelayProbeState::Checking,
                    latency_ms: None,
                })
                .collect(),
        );
        for relay in relays {
            probe_single_relay(relay, relay_probes.clone());
        }
    }

    fn set_relay_probe(
        relay_probes: UseStateHandle<Vec<RelayProbe>>,
        relay: &str,
        state: RelayProbeState,
        latency_ms: Option<u32>,
    ) {
        let mut next = (*relay_probes).clone();
        if let Some(existing) = next.iter_mut().find(|probe| probe.relay == relay) {
            existing.state = state;
            existing.latency_ms = latency_ms;
            relay_probes.set(next);
        }
    }

    fn probe_single_relay(relay: String, relay_probes: UseStateHandle<Vec<RelayProbe>>) {
        let ws = match WebSocket::new(&relay) {
            Ok(socket) => Rc::new(socket),
            Err(_) => {
                set_relay_probe(relay_probes, &relay, RelayProbeState::Unreachable, None);
                return;
            }
        };

        let started_at = Date::now();
        let settled = Rc::new(RefCell::new(false));
        let listeners = Rc::new(RefCell::new(Vec::<EventListener>::new()));
        let timeout_handle = Rc::new(RefCell::new(None::<Timeout>));

        let finish = {
            let settled = settled.clone();
            let relay = relay.clone();
            let relay_probes = relay_probes.clone();
            let listeners = listeners.clone();
            let timeout_handle = timeout_handle.clone();
            let ws = ws.clone();
            Rc::new(move |state: RelayProbeState, latency_ms: Option<u32>| {
                if *settled.borrow() {
                    return;
                }
                *settled.borrow_mut() = true;
                set_relay_probe(relay_probes.clone(), &relay, state, latency_ms);
                listeners.borrow_mut().clear();
                timeout_handle.borrow_mut().take();
                let _ = ws.close();
            })
        };

        {
            let finish = finish.clone();
            let ws = ws.clone();
            listeners
                .borrow_mut()
                .push(EventListener::new(ws.as_ref(), "open", move |_| {
                    let latency = (Date::now() - started_at).max(0.0) as u32;
                    finish(RelayProbeState::Reachable, Some(latency));
                }));
        }
        {
            let finish = finish.clone();
            let ws = ws.clone();
            listeners
                .borrow_mut()
                .push(EventListener::new(ws.as_ref(), "error", move |_| {
                    finish(RelayProbeState::Unreachable, None);
                }));
        }
        {
            let finish = finish.clone();
            let ws = ws.clone();
            listeners
                .borrow_mut()
                .push(EventListener::new(ws.as_ref(), "close", move |_| {
                    finish(RelayProbeState::Unreachable, None);
                }));
        }
        {
            let finish = finish.clone();
            timeout_handle
                .borrow_mut()
                .replace(Timeout::new(4_000, move || {
                    finish(RelayProbeState::Unreachable, None);
                }));
        }
    }

    fn prepare_amber_session(relays: &[String]) -> Result<(String, String), String> {
        let session_keys = Keys::generate();
        let relay_urls = relay_urls_from_strings(relays);
        if relay_urls.is_empty() {
            return Err("add at least one valid relay first".to_string());
        }

        let uri =
            NostrConnectURI::client(session_keys.public_key(), relay_urls, "niplock").to_string();
        let secret = Uuid::new_v4().to_string();
        let uri = append_query_param(&uri, "secret", &secret);
        let uri = append_query_param(&uri, "perms", AMBER_NIP46_PERMS);
        let app_key = session_keys
            .secret_key()
            .to_bech32()
            .map_err(|err| err.to_string())?;

        Ok((uri.clone(), format!("{uri}::appkey={app_key}")))
    }

    fn append_query_param(uri: &str, key: &str, value: &str) -> String {
        let separator = if uri.contains('?') { "&" } else { "?" };
        format!("{uri}{separator}{key}={value}")
    }

    fn open_external_uri(uri: &str) {
        let Some(win) = window() else {
            return;
        };

        if uri.starts_with("nostrconnect://") || uri.starts_with("bunker://") {
            let _ = win.location().set_href(uri);
            return;
        }

        let opened = win.open_with_url_and_target(uri, "_blank").ok().flatten();
        if opened.is_none() {
            let _ = win.location().set_href(uri);
        }
    }

    fn push_amber_debug(debug: UseStateHandle<Vec<String>>, message: String) {
        let mut next = (*debug).clone();
        next.push(message);
        if next.len() > 12 {
            let excess = next.len() - 12;
            next.drain(0..excess);
        }
        debug.set(next);
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_amber_unlock(
        signer_credential: String,
        relays: Vec<String>,
        entries_state: UseStateHandle<Vec<PasswordEntry>>,
        signer_credential_state: UseStateHandle<String>,
        active_npub: UseStateHandle<Option<String>>,
        unlocked: UseStateHandle<bool>,
        unlock_error: UseStateHandle<Option<String>>,
        unlock_panel_open: UseStateHandle<bool>,
        sync_state: UseStateHandle<SyncState>,
        sync_detail: UseStateHandle<Option<String>>,
        last_sync: UseStateHandle<Option<String>>,
        sync_in_flight: UseStateHandle<bool>,
        relay_copies_by_entry: UseStateHandle<HashMap<String, usize>>,
        relay_copy_target: usize,
        live_sync: Rc<RefCell<Option<NostrSync>>>,
        amber_debug: UseStateHandle<Vec<String>>,
    ) {
        if *sync_in_flight {
            push_amber_debug(
                amber_debug,
                "Amber: unlock already in progress; waiting for approval".to_string(),
            );
            unlock_error.set(Some(
                "Waiting for Amber approval. Approve niplock in Amber.".to_string(),
            ));
            return;
        }

        sync_in_flight.set(true);
        sync_state.set(SyncState::Syncing);
        sync_detail.set(Some("Waiting for Amber approval".to_string()));
        push_amber_debug(
            amber_debug.clone(),
            format!(
                "Amber: starting unlock with {} configured relay(s)",
                relays.len()
            ),
        );
        let timed_out = Rc::new(RefCell::new(false));
        let timeout_flag = timed_out.clone();
        let sync_state_timeout = sync_state.clone();
        let sync_in_flight_timeout = sync_in_flight.clone();
        let sync_detail_timeout = sync_detail.clone();
        let unlock_error_timeout = unlock_error.clone();
        let amber_debug_timeout = amber_debug.clone();
        let watchdog = Timeout::new(45_000, move || {
            if *sync_in_flight_timeout {
                *timeout_flag.borrow_mut() = true;
                push_amber_debug(
                    amber_debug_timeout.clone(),
                    "Amber: watchdog timeout while unlock task was still running".to_string(),
                );
                unlock_error_timeout
                    .set(Some("Amber sync timeout. Tap Sync to retry.".to_string()));
                sync_state_timeout.set(SyncState::Error(
                    "Amber sync timeout. Tap Sync to retry.".to_string(),
                ));
                sync_detail_timeout.set(Some(
                    "Timed out while waiting for relay/signer responses".to_string(),
                ));
                sync_in_flight_timeout.set(false);
            }
        });
        unlock_error.set(Some(
            "Waiting for Amber approval. Approve niplock in Amber.".to_string(),
        ));

        spawn_local(async move {
            let _watchdog = watchdog;
            push_amber_debug(
                amber_debug.clone(),
                "Amber: polling relays for approval response".to_string(),
            );
            let signer_credential = match wait_for_amber_approval(
                signer_credential,
                relays.clone(),
                amber_debug.clone(),
            )
            .await
            {
                Ok(v) => v,
                Err(err) => {
                    push_amber_debug(
                        amber_debug.clone(),
                        format!("Amber: approval failed: {err}"),
                    );
                    unlock_error.set(Some(format!("Amber approval failed: {err}")));
                    sync_state.set(SyncState::Error("Amber approval failed".to_string()));
                    sync_detail.set(Some("Amber approval failed".to_string()));
                    sync_in_flight.set(false);
                    return;
                }
            };

            let mut signer_credential = signer_credential;
            push_amber_debug(amber_debug.clone(), "Amber: creating signer".to_string());
            let mut signer = match signer_from_input(&signer_credential) {
                Ok(v) => v,
                Err(err) => {
                    push_amber_debug(
                        amber_debug.clone(),
                        format!("Amber: invalid session: {err}"),
                    );
                    unlock_error.set(Some(format!("Invalid Amber session: {err}")));
                    sync_state.set(SyncState::Error("Amber session failed".to_string()));
                    sync_detail.set(Some("Amber session was rejected".to_string()));
                    sync_in_flight.set(false);
                    return;
                }
            };
            if *timed_out.borrow() {
                return;
            }

            push_amber_debug(
                amber_debug.clone(),
                "Amber: requesting public key from signer".to_string(),
            );
            let public_key = match signer.get_public_key().await {
                Ok(v) => {
                    push_amber_debug(
                        amber_debug.clone(),
                        "Amber: signer public key returned".to_string(),
                    );
                    v
                }
                Err(err) => {
                    push_amber_debug(
                        amber_debug.clone(),
                        format!("Amber: public key failed; replaying approval: {err}"),
                    );
                    let replayed = amber_credential_with_replayed_connect(
                        signer_credential.clone(),
                        relays.clone(),
                        amber_debug.clone(),
                    )
                    .await;
                    if let Ok(next_credential) = replayed {
                        if next_credential != signer_credential {
                            push_amber_debug(
                                amber_debug.clone(),
                                "Amber: replay produced bunker credential; retrying public key"
                                    .to_string(),
                            );
                            if let Ok(next_signer) = signer_from_input(&next_credential) {
                                match next_signer.get_public_key().await {
                                    Ok(v) => {
                                        push_amber_debug(
                                            amber_debug.clone(),
                                            "Amber: signer public key returned after replay"
                                                .to_string(),
                                        );
                                        signer_credential = next_credential;
                                        signer = next_signer;
                                        v
                                    }
                                    Err(retry_err) => {
                                        push_amber_debug(
                                            amber_debug.clone(),
                                            format!(
                                                "Amber: replay public key retry failed: {retry_err}"
                                            ),
                                        );
                                        unlock_error.set(Some(format!(
                                            "Amber approval failed: {err}; replay retry failed: {retry_err}"
                                        )));
                                        sync_state.set(SyncState::Error(
                                            "Amber approval failed".to_string(),
                                        ));
                                        sync_detail
                                            .set(Some("Amber approval replay failed".to_string()));
                                        sync_in_flight.set(false);
                                        return;
                                    }
                                }
                            } else {
                                push_amber_debug(
                                    amber_debug.clone(),
                                    "Amber: replay credential could not create signer".to_string(),
                                );
                                unlock_error.set(Some(format!("Amber approval failed: {err}")));
                                sync_state
                                    .set(SyncState::Error("Amber approval failed".to_string()));
                                sync_detail.set(Some("Amber approval replay failed".to_string()));
                                sync_in_flight.set(false);
                                return;
                            }
                        } else {
                            push_amber_debug(
                                amber_debug.clone(),
                                "Amber: replay did not find a usable approval".to_string(),
                            );
                            unlock_error.set(Some(format!("Amber approval failed: {err}")));
                            sync_state.set(SyncState::Error("Amber approval failed".to_string()));
                            sync_detail.set(Some("Amber approval replay failed".to_string()));
                            sync_in_flight.set(false);
                            return;
                        }
                    } else {
                        push_amber_debug(
                            amber_debug.clone(),
                            "Amber: replay approval lookup failed".to_string(),
                        );
                        unlock_error.set(Some(format!("Amber approval failed: {err}")));
                        sync_state.set(SyncState::Error("Amber approval failed".to_string()));
                        sync_detail.set(Some("Amber approval replay lookup failed".to_string()));
                        sync_in_flight.set(false);
                        return;
                    }
                }
            };
            push_amber_debug(
                amber_debug.clone(),
                "Amber: approved; opening vault before background sync".to_string(),
            );
            let public_npub = public_key.to_bech32().ok();
            set_active_npub_storage(public_npub.as_deref());
            signer_credential_state.set(signer_credential.clone());
            active_npub.set(public_npub.clone());
            unlocked.set(true);
            unlock_panel_open.set(false);
            unlock_error.set(None);

            unlock_error.set(Some("Amber approved. Connecting relays...".to_string()));
            sync_detail.set(Some("Connecting relays".to_string()));
            push_amber_debug(
                amber_debug.clone(),
                "Amber: approved; creating Nostr sync client".to_string(),
            );

            let sync = match NostrSync::new_with_signer(signer, relays).await {
                Ok(v) => {
                    push_amber_debug(amber_debug.clone(), "Amber: relay client ready".to_string());
                    v
                }
                Err(err) => {
                    push_amber_debug(
                        amber_debug.clone(),
                        format!("Amber: relay client failed: {err}"),
                    );
                    signer_credential_state.set(signer_credential);
                    active_npub.set(public_key.to_bech32().ok());
                    unlocked.set(true);
                    unlock_panel_open.set(false);
                    unlock_error.set(Some(format!("Amber connected; relay sync failed: {err}")));
                    sync_state.set(SyncState::Error("relay connect failed".to_string()));
                    sync_detail.set(Some(format!("Relay connect failed: {err}")));
                    sync_in_flight.set(false);
                    return;
                }
            };

            unlock_error.set(Some("Amber approved. Syncing vault...".to_string()));
            sync_detail.set(Some("Downloading and merging vault entries".to_string()));
            let cached_entries = merge_entry_lists(&*entries_state, &load_entries());
            entries_state.set(cached_entries.clone());
            let local = to_map(&cached_entries);
            push_amber_debug(
                amber_debug.clone(),
                format!("Amber: syncing vault with {} local entries", local.len()),
            );
            let amber_debug_progress = amber_debug.clone();
            let sync_detail_progress = sync_detail.clone();
            match sync
                .sync_with_progress_target(&local, relay_copy_target, move |message| {
                    sync_detail_progress.set(Some(message.clone()));
                    push_amber_debug(amber_debug_progress.clone(), message);
                })
                .await
            {
                Ok((merged, summary)) => {
                    if *timed_out.borrow() {
                        push_amber_debug(
                            amber_debug.clone(),
                            "Amber: sync completed after watchdog; closing unlock panel"
                                .to_string(),
                        );
                    }
                    push_amber_debug(
                        amber_debug.clone(),
                        format!("Amber: sync returned {} merged entries", merged.len()),
                    );
                    let next = from_map(merged);
                    save_entries(&next);
                    entries_state.set(next);
                    relay_copies_by_entry.set(summary.entry_relay_copies);
                    last_sync.set(Some(Utc::now().to_rfc3339()));
                    *live_sync.borrow_mut() = Some(sync);
                    signer_credential_state.set(signer_credential);
                    active_npub.set(public_npub.clone());
                    unlocked.set(true);
                    unlock_panel_open.set(false);
                    unlock_error.set(None);
                    sync_state.set(SyncState::Idle);
                    sync_detail.set(None);
                    sync_in_flight.set(false);
                }
                Err(err) => {
                    if *timed_out.borrow() {
                        push_amber_debug(
                            amber_debug.clone(),
                            "Amber: sync failed after watchdog; showing final error".to_string(),
                        );
                    }
                    push_amber_debug(amber_debug.clone(), format!("Amber: sync failed: {err}"));
                    *live_sync.borrow_mut() = Some(sync);
                    signer_credential_state.set(signer_credential);
                    active_npub.set(public_npub);
                    unlocked.set(true);
                    unlock_panel_open.set(false);
                    unlock_error.set(Some(format!("Amber connected; sync failed: {err}")));
                    sync_state.set(SyncState::Error(format!("sync failed: {err}")));
                    sync_detail.set(Some(format!("Sync failed: {err}")));
                    sync_in_flight.set(false);
                }
            }
        });
    }

    async fn wait_for_amber_approval(
        signer_credential: String,
        relays: Vec<String>,
        amber_debug: UseStateHandle<Vec<String>>,
    ) -> Result<String, String> {
        let Some((uri_part, _app_key)) = signer_credential.split_once("::appkey=") else {
            return Ok(signer_credential);
        };

        if !uri_part.starts_with("nostrconnect://") {
            return Ok(signer_credential);
        }

        for attempt in 0..AMBER_APPROVAL_POLL_ATTEMPTS {
            if attempt == 0 || attempt % 5 == 4 {
                push_amber_debug(
                    amber_debug.clone(),
                    format!(
                        "Amber: approval poll {}/{}",
                        attempt + 1,
                        AMBER_APPROVAL_POLL_ATTEMPTS
                    ),
                );
            }
            let next = amber_credential_with_replayed_connect(
                signer_credential.clone(),
                relays.clone(),
                amber_debug.clone(),
            )
            .await?;
            if next != signer_credential {
                push_amber_debug(
                    amber_debug.clone(),
                    "Amber: approval response found".to_string(),
                );
                return Ok(next);
            }
            if attempt + 1 < AMBER_APPROVAL_POLL_ATTEMPTS {
                TimeoutFuture::new(AMBER_APPROVAL_POLL_INTERVAL_MS).await;
            }
        }

        Err("timed out waiting for Amber approval".to_string())
    }

    async fn amber_credential_with_replayed_connect(
        signer_credential: String,
        relays: Vec<String>,
        amber_debug: UseStateHandle<Vec<String>>,
    ) -> Result<String, String> {
        let base_credential = signer_credential
            .strip_suffix(PREAPPROVED_NIP46_MARKER)
            .unwrap_or(&signer_credential);
        let Some((uri_part, app_key)) = base_credential.split_once("::appkey=") else {
            return Ok(signer_credential);
        };

        if !uri_part.starts_with("nostrconnect://") {
            return Ok(signer_credential);
        }

        let uri = NostrConnectURI::parse(uri_part).map_err(|err| err.to_string())?;
        let app_keys = Keys::parse(app_key).map_err(|err| err.to_string())?;
        let expected_secret = nostr_connect_query_param(uri_part, "secret");
        let Some((remote_signer_public_key, secret)) =
            find_replayed_amber_connect(&app_keys, relays, expected_secret, amber_debug.clone())
                .await?
        else {
            return Ok(signer_credential);
        };

        let relay_urls = uri.relays().to_vec();
        let bunker_uri = NostrConnectURI::Bunker {
            remote_signer_public_key,
            relays: relay_urls,
            secret,
        };

        Ok(format!(
            "{bunker_uri}::appkey={app_key}{PREAPPROVED_NIP46_MARKER}"
        ))
    }

    async fn find_replayed_amber_connect(
        app_keys: &Keys,
        relays: Vec<String>,
        expected_secret: Option<String>,
        amber_debug: UseStateHandle<Vec<String>>,
    ) -> Result<Option<(nostr_sdk::prelude::PublicKey, Option<String>)>, String> {
        let client = Client::default();
        let relays = sanitize_relays(relays);
        push_amber_debug(
            amber_debug.clone(),
            format!(
                "Amber: checking {} relay(s) for NIP-46 events",
                relays.len()
            ),
        );
        for relay in &relays {
            client
                .add_relay(relay)
                .await
                .map_err(|err| err.to_string())?;
        }

        client.connect().await;
        client
            .wait_for_connection(std::time::Duration::from_secs(5))
            .await;
        push_amber_debug(
            amber_debug.clone(),
            "Amber: relay check connected; fetching NIP-46 events".to_string(),
        );

        let filter = Filter::new()
            .kind(Kind::NostrConnect)
            .pubkey(app_keys.public_key())
            .limit(20);
        let events = client
            .fetch_events(filter, std::time::Duration::from_secs(8))
            .await
            .map_err(|err| err.to_string())?;
        push_amber_debug(
            amber_debug.clone(),
            format!("Amber: fetched {} possible approval event(s)", events.len()),
        );

        let mut decrypted = 0usize;
        let mut parsed = 0usize;
        for event in events.iter() {
            let Ok(message) =
                nip44::decrypt(app_keys.secret_key(), &event.pubkey, event.content.as_str())
            else {
                continue;
            };
            decrypted += 1;
            let Ok(message) = NostrConnectMessage::from_json(message) else {
                continue;
            };
            parsed += 1;

            let request_id = message.id().to_string();
            let should_ack = message.is_request();
            if let Some((remote_signer_public_key, secret)) =
                amber_connect_approval_signer(message, event.pubkey, expected_secret.as_deref())
            {
                push_amber_debug(
                    amber_debug.clone(),
                    format!(
                        "Amber: matched approval event; request_ack={should_ack}, secret={}",
                        secret.is_some()
                    ),
                );
                if should_ack {
                    let response = NostrConnectMessage::response(
                        request_id,
                        NostrConnectResponse::with_result(
                            nostr_sdk::nips::nip46::ResponseResult::Ack,
                        ),
                    );
                    if let Ok(event) =
                        EventBuilder::nostr_connect(app_keys, remote_signer_public_key, response)
                            .and_then(|builder| builder.sign_with_keys(app_keys))
                    {
                        let _ = client.send_event_to(relays.clone(), &event).await;
                        push_amber_debug(
                            amber_debug.clone(),
                            "Amber: sent ACK for signer connect request".to_string(),
                        );
                    }
                }
                client.shutdown().await;
                return Ok(Some((remote_signer_public_key, secret)));
            }
        }

        push_amber_debug(
            amber_debug.clone(),
            format!("Amber: no approval matched; decrypted {decrypted}, parsed {parsed}"),
        );
        client.shutdown().await;
        Ok(None)
    }

    fn amber_connect_approval_signer(
        message: NostrConnectMessage,
        signer_public_key: nostr_sdk::prelude::PublicKey,
        expected_secret: Option<&str>,
    ) -> Option<(nostr_sdk::prelude::PublicKey, Option<String>)> {
        match message {
            NostrConnectMessage::Response {
                result: Some(result),
                error: None,
                ..
            } if result == "ack" => Some((signer_public_key, None)),
            NostrConnectMessage::Response {
                result: Some(result),
                error: None,
                ..
            } if expected_secret == Some(result.as_str()) => {
                Some((signer_public_key, Some(result)))
            }
            NostrConnectMessage::Request { method, params, .. } => {
                match NostrConnectRequest::from_message(method, params) {
                    Ok(NostrConnectRequest::Connect {
                        remote_signer_public_key,
                        secret,
                    }) if remote_signer_public_key == signer_public_key => {
                        Some((signer_public_key, secret))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn nostr_connect_query_param(uri: &str, key: &str) -> Option<String> {
        let query = uri.split_once('?')?.1;
        for pair in query.split('&') {
            if let Some((candidate, value)) = pair.split_once('=') {
                if candidate == key {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    fn spawn_sync(
        signer_credential: String,
        local_entries: Vec<PasswordEntry>,
        relays: Vec<String>,
        entries_state: UseStateHandle<Vec<PasswordEntry>>,
        sync_state: UseStateHandle<SyncState>,
        sync_detail: UseStateHandle<Option<String>>,
        last_sync: UseStateHandle<Option<String>>,
        sync_in_flight: UseStateHandle<bool>,
        relay_copies_by_entry: UseStateHandle<HashMap<String, usize>>,
        relay_copy_target: usize,
        live_sync: Rc<RefCell<Option<NostrSync>>>,
    ) {
        if *sync_in_flight {
            return;
        }

        sync_in_flight.set(true);
        sync_state.set(SyncState::Syncing);
        sync_detail.set(Some("Starting sync".to_string()));
        let timed_out = Rc::new(RefCell::new(false));
        let timeout_flag = timed_out.clone();
        let sync_state_timeout = sync_state.clone();
        let sync_in_flight_timeout = sync_in_flight.clone();
        let sync_detail_timeout = sync_detail.clone();
        let watchdog = Timeout::new(35_000, move || {
            if *sync_in_flight_timeout {
                *timeout_flag.borrow_mut() = true;
                sync_state_timeout.set(SyncState::Error(
                    "sync timeout: approve in Amber, then tap Sync".to_string(),
                ));
                sync_detail_timeout.set(Some(
                    "Timed out waiting for relay/signer responses".to_string(),
                ));
                sync_in_flight_timeout.set(false);
            }
        });

        spawn_local(async move {
            let _watchdog = watchdog;
            if signer_credential.trim().is_empty() {
                sync_state.set(SyncState::Error(
                    "set signer credential to enable NIP-17 sync".to_string(),
                ));
                sync_detail.set(None);
                sync_in_flight.set(false);
                return;
            }

            let sync = if let Some(existing) = live_sync.borrow().clone() {
                existing
            } else {
                let signer = match signer_from_input(&signer_credential) {
                    Ok(v) => v,
                    Err(err) => {
                        sync_state.set(SyncState::Error(format!("invalid signer: {err}")));
                        sync_detail.set(Some(format!("Invalid signer: {err}")));
                        sync_in_flight.set(false);
                        return;
                    }
                };

                match NostrSync::new_with_signer(signer, relays).await {
                    Ok(v) => {
                        *live_sync.borrow_mut() = Some(v.clone());
                        v
                    }
                    Err(err) => {
                        sync_state.set(SyncState::Error(format!("relay connect failed: {err}")));
                        sync_detail.set(Some(format!("Relay connect failed: {err}")));
                        sync_in_flight.set(false);
                        return;
                    }
                }
            };

            let local = to_map(&local_entries);
            let sync_detail_progress = sync_detail.clone();
            match sync
                .sync_with_progress_target(&local, relay_copy_target, move |message| {
                    sync_detail_progress.set(Some(message));
                })
                .await
            {
                Ok((merged, summary)) => {
                    if *timed_out.borrow() {
                        return;
                    }
                    let next = from_map(merged);
                    save_entries(&next);
                    entries_state.set(next);
                    relay_copies_by_entry.set(summary.entry_relay_copies);
                    last_sync.set(Some(Utc::now().to_rfc3339()));
                    sync_state.set(SyncState::Idle);
                    sync_detail.set(None);
                }
                Err(err) => {
                    if *timed_out.borrow() {
                        return;
                    }
                    *live_sync.borrow_mut() = None;
                    sync_state.set(SyncState::Error(format!("sync failed: {err}")));
                    sync_detail.set(Some(format!("Sync failed: {err}")));
                }
            }
            sync_in_flight.set(false);
        });
    }

    fn copy_to_clipboard(
        value: String,
        copy_notice: UseStateHandle<Option<String>>,
        success_message: String,
    ) {
        let Some(win) = window() else {
            copy_notice.set(Some("Copy failed".to_string()));
            return;
        };

        let clipboard = win.navigator().clipboard();
        let promise = clipboard.write_text(&value);
        spawn_local(async move {
            if JsFuture::from(promise).await.is_ok() {
                copy_notice.set(Some(success_message));
            } else {
                copy_notice.set(Some("Copy failed".to_string()));
            }
        });
    }

    fn save_entries(entries: &[PasswordEntry]) {
        if let Some(storage) = local_storage() {
            if let Ok(payload) = serde_json::to_string(entries) {
                let key = current_storage_key();
                let _ = storage.set_item(key.as_str(), &payload);
            }
        }
    }

    fn load_entries() -> Vec<PasswordEntry> {
        let Some(storage) = local_storage() else {
            return Vec::new();
        };
        let key = current_storage_key();
        let parsed = storage
            .get_item(key.as_str())
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str::<Vec<PasswordEntry>>(&raw).ok());

        let parsed = if let Some(parsed) = parsed {
            parsed
        } else if key != STORAGE_KEY {
            let legacy = storage
                .get_item(STORAGE_KEY)
                .ok()
                .flatten()
                .and_then(|raw| serde_json::from_str::<Vec<PasswordEntry>>(&raw).ok())
                .unwrap_or_default();
            if !legacy.is_empty() {
                if let Ok(payload) = serde_json::to_string(&legacy) {
                    let _ = storage.set_item(key.as_str(), &payload);
                }
                let _ = storage.remove_item(STORAGE_KEY);
            }
            legacy
        } else {
            Vec::new()
        };

        from_map(to_map(&parsed))
    }

    fn set_active_npub_storage(npub: Option<&str>) {
        let Some(storage) = local_storage() else {
            return;
        };
        if let Some(npub) = npub {
            let _ = storage.set_item(ACTIVE_NPUB_STORAGE_KEY, npub);
        } else {
            let _ = storage.remove_item(ACTIVE_NPUB_STORAGE_KEY);
        }
    }

    fn current_storage_key() -> String {
        let Some(storage) = local_storage() else {
            return STORAGE_KEY.to_string();
        };
        let Ok(active) = storage.get_item(ACTIVE_NPUB_STORAGE_KEY) else {
            return STORAGE_KEY.to_string();
        };
        let Some(active) = active else {
            return STORAGE_KEY.to_string();
        };
        let active = active.trim();
        if active.is_empty() {
            STORAGE_KEY.to_string()
        } else {
            format!("{STORAGE_KEY}::{active}")
        }
    }

    fn load_relays() -> Vec<String> {
        let Some(storage) = local_storage() else {
            return default_relay_strings();
        };
        let Ok(Some(raw)) = storage.get_item(RELAYS_STORAGE_KEY) else {
            return default_relay_strings();
        };
        let Ok(parsed) = serde_json::from_str::<Vec<String>>(&raw) else {
            return default_relay_strings();
        };

        let relays = sanitize_relays(parsed);
        if relays.is_empty() {
            default_relay_strings()
        } else {
            save_relays(&relays);
            relays
        }
    }

    fn save_relays(relays: &[String]) {
        if let Some(storage) = local_storage() {
            if let Ok(payload) = serde_json::to_string(&sanitize_relays(relays.to_vec())) {
                let _ = storage.set_item(RELAYS_STORAGE_KEY, &payload);
            }
        }
    }

    fn load_relay_copy_target() -> usize {
        let Some(storage) = local_storage() else {
            return DEFAULT_RELAY_COPY_TARGET;
        };
        let Ok(Some(raw)) = storage.get_item(RELAY_COPY_TARGET_STORAGE_KEY) else {
            return DEFAULT_RELAY_COPY_TARGET;
        };
        raw.trim()
            .parse::<usize>()
            .ok()
            .map(sanitize_relay_copy_target)
            .unwrap_or(DEFAULT_RELAY_COPY_TARGET)
    }

    fn save_relay_copy_target(target: usize) {
        if let Some(storage) = local_storage() {
            let _ = storage.set_item(
                RELAY_COPY_TARGET_STORAGE_KEY,
                sanitize_relay_copy_target(target).to_string().as_str(),
            );
        }
    }

    fn sanitize_relay_copy_target(target: usize) -> usize {
        target.clamp(1, MAX_SYNC_RELAYS)
    }

    fn sanitize_relays(relays: Vec<String>) -> Vec<String> {
        let mut clean = Vec::<String>::new();
        for relay in relays {
            let Some(normalized) = normalize_relay_url(&relay) else {
                continue;
            };
            if clean
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&normalized))
            {
                continue;
            }
            clean.push(normalized);
            if clean.len() >= MAX_SYNC_RELAYS {
                break;
            }
        }
        clean
    }

    fn normalize_relay_url(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if !(trimmed.starts_with("wss://") || trimmed.starts_with("ws://")) {
            return None;
        }
        RelayUrl::parse(trimmed).ok().map(|relay| relay.to_string())
    }

    fn local_storage() -> Option<web_sys::Storage> {
        window().and_then(|w| w.local_storage().ok().flatten())
    }

    fn to_map(entries: &[PasswordEntry]) -> HashMap<String, PasswordEntry> {
        entries
            .iter()
            .cloned()
            .map(|entry| (entry.id.clone(), entry))
            .collect()
    }

    fn merge_entry_lists(left: &[PasswordEntry], right: &[PasswordEntry]) -> Vec<PasswordEntry> {
        let mut merged = to_map(left);
        for entry in right {
            let entry = PasswordEntry::merge_prefer_newer(merged.get(&entry.id), entry.clone());
            merged.insert(entry.id.clone(), entry);
        }
        from_map(merged)
    }

    fn from_map(map: HashMap<String, PasswordEntry>) -> Vec<PasswordEntry> {
        let mut entries: Vec<PasswordEntry> = map.into_values().collect();
        entries.sort_by(|a, b| {
            a.service
                .to_ascii_lowercase()
                .cmp(&b.service.to_ascii_lowercase())
                .then(
                    a.username
                        .to_ascii_lowercase()
                        .cmp(&b.username.to_ascii_lowercase()),
                )
        });
        entries
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::run();
}

// probe
