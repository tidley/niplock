#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("passwd-web is a wasm binary. Build with: trunk serve --open");
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    use chrono::{DateTime, Utc};
    use gloo_events::EventListener;
    use js_sys::Math;
    use nostr_sdk::prelude::ToBech32;
    use passwd::model::PasswordEntry;
    use passwd::nostr_sync::{NostrSync, signer_from_input};
    use uuid::Uuid;
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{HtmlInputElement, HtmlTextAreaElement, window};
    use yew::prelude::*;

    const STORAGE_KEY: &str = "passwd.vault.v1";
    const DEFAULT_RELAYS: [&str; 2] = ["wss://nip17.tomdwyer.uk", "wss://nip17.com"];

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
  .table th:nth-child(4), .table td:nth-child(4) { display: none; }
  .table th, .table td { padding: 8px 6px; font-size: 0.9rem; }
  .table th { font-size: 0.62rem; }
  .top { padding: 0 8px; }
  .search { padding: 8px 10px; font-size: 0.85rem; }
  .btn { padding: 7px 8px; font-size: 0.78rem; }
  .unlock { font-size: 0.68rem; padding: 7px 8px; }
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
        let unlocked = use_state(|| false);
        let mobile_menu_open = use_state(|| false);

        let sync_state = use_state(|| SyncState::Idle);
        let last_sync = use_state(|| None::<String>);
        let sync_in_flight = use_state(|| false);
        let live_sync = use_mut_ref(|| None::<NostrSync>);
        let live_subscription_id = use_mut_ref(|| None::<nostr_sdk::prelude::SubscriptionId>);
        let live_listener_running = use_mut_ref(|| false);
        let copy_notice = use_state(|| None::<String>);

        let gen_len = use_state(|| 18usize);
        let gen_upper = use_state(|| true);
        let gen_lower = use_state(|| true);
        let gen_numbers = use_state(|| true);
        let gen_symbols = use_state(|| true);
        let generated = use_state(|| generate_password(18, true, true, true, true));

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
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            use_effect_with((), move |_| {
                let doc_listener = window().and_then(|w| w.document()).map(|doc| {
                    let entries = entries.clone();
                    let signer_credential = signer_credential.clone();
                    let sync_state = sync_state.clone();
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    let live_sync = live_sync.clone();
                    let unlocked = unlocked.clone();
                    EventListener::new(&doc, "visibilitychange", move |_| {
                        if *unlocked {
                            if let Some(document) = window().and_then(|w| w.document()) {
                                if document.hidden() {
                                    spawn_sync(
                                        (*signer_credential).clone(),
                                        (*entries).clone(),
                                        entries.clone(),
                                        sync_state.clone(),
                                        last_sync.clone(),
                                        sync_in_flight.clone(),
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
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    let live_sync = live_sync.clone();
                    let unlocked = unlocked.clone();
                    EventListener::new(&win, "pagehide", move |_| {
                        if *unlocked {
                            spawn_sync(
                                (*signer_credential).clone(),
                                (*entries).clone(),
                                entries.clone(),
                                sync_state.clone(),
                                last_sync.clone(),
                                sync_in_flight.clone(),
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
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            let unlocked = unlocked.clone();
            Callback::from(move |_| {
                if *live_listener_running.borrow() {
                    return;
                }
                *live_listener_running.borrow_mut() = true;

                let signer_credential = signer_credential.clone();
                let entries = entries.clone();
                let sync_state = sync_state.clone();
                let last_sync = last_sync.clone();
                let sync_in_flight = sync_in_flight.clone();
                let live_sync = live_sync.clone();
                let live_subscription_id = live_subscription_id.clone();
                let live_listener_running = live_listener_running.clone();
                let unlocked = unlocked.clone();
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

                            match NostrSync::new_with_signer(
                                signer,
                                DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect(),
                            )
                            .await
                            {
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

                        spawn_sync(
                            (*signer_credential).clone(),
                            (*entries).clone(),
                            entries.clone(),
                            sync_state.clone(),
                            last_sync.clone(),
                            sync_in_flight.clone(),
                            live_sync.clone(),
                        );
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
            let entries = entries.clone();
            let selected_id = selected_id.clone();
            let editor_open = editor_open.clone();
            let live_sync = live_sync.clone();
            let live_subscription_id = live_subscription_id.clone();
            let live_listener_running = live_listener_running.clone();
            Callback::from(move |_| {
                if *unlocked {
                    unlocked.set(false);
                    unlock_panel_open.set(false);
                    unlock_error.set(None);
                    active_npub.set(None);
                    entries.set(vec![]);
                    selected_id.set(None);
                    editor_open.set(false);
                    *live_listener_running.borrow_mut() = false;
                    *live_subscription_id.borrow_mut() = None;
                    {
                        let live_sync = live_sync.clone();
                        spawn_local(async move {
                            let current = live_sync.borrow_mut().take();
                            if let Some(sync) = current {
                                sync.shutdown().await;
                            }
                        });
                    }
                } else {
                    unlock_panel_open.set(!*unlock_panel_open);
                }
            })
        };

        let on_unlock_method_nsec = {
            let unlock_method = unlock_method.clone();
            Callback::from(move |_| unlock_method.set(UnlockMethod::Nsec))
        };
        let on_unlock_method_amber = {
            let unlock_method = unlock_method.clone();
            Callback::from(move |_| unlock_method.set(UnlockMethod::Amber))
        };
        let on_unlock_method_nip07 = {
            let unlock_method = unlock_method.clone();
            Callback::from(move |_| unlock_method.set(UnlockMethod::Nip07))
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
            let unlocked = unlocked.clone();
            let unlock_error = unlock_error.clone();
            let unlock_panel_open = unlock_panel_open.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let live_sync = live_sync.clone();
            let start_live_listener = start_live_listener.clone();
            Callback::from(move |_| {
                let credential = match &*unlock_method {
                    UnlockMethod::Nip07 => "nip07".to_string(),
                    UnlockMethod::Nsec | UnlockMethod::Amber => unlock_input.trim().to_string(),
                };

                match signer_from_input(&credential) {
                    Ok(signer) => {
                        signer_credential.set(credential.clone());
                        {
                            let active_npub = active_npub.clone();
                            spawn_local(async move {
                                let next = match signer.get_public_key().await {
                                    Ok(pubkey) => pubkey.to_bech32().ok(),
                                    Err(_) => None,
                                };
                                active_npub.set(next);
                            });
                        }
                        unlocked.set(true);
                        unlock_error.set(None);
                        unlock_panel_open.set(false);
                        spawn_sync(
                            credential,
                            (*entries).clone(),
                            entries.clone(),
                            sync_state.clone(),
                            last_sync.clone(),
                            sync_in_flight.clone(),
                            live_sync.clone(),
                        );
                        start_live_listener.emit(());
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
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            Callback::from(move |_| {
                if *unlocked {
                    spawn_sync(
                        (*signer_credential).clone(),
                        (*entries).clone(),
                        entries.clone(),
                        sync_state.clone(),
                        last_sync.clone(),
                        sync_in_flight.clone(),
                        live_sync.clone(),
                    );
                }
            })
        };

        let on_entries_modified = {
            let signer_credential = signer_credential.clone();
            let entries = entries.clone();
            let sync_state = sync_state.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            let live_sync = live_sync.clone();
            let unlocked = unlocked.clone();
            Callback::from(move |current_entries: Vec<PasswordEntry>| {
                if *unlocked {
                    spawn_sync(
                        (*signer_credential).clone(),
                        current_entries,
                        entries.clone(),
                        sync_state.clone(),
                        last_sync.clone(),
                        sync_in_flight.clone(),
                        live_sync.clone(),
                    );
                }
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
                selected_id.set(Some(id));
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
            SyncState::Syncing => "Syncing".to_string(),
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

        html! {
            <>
                <style>{CSS}</style>
                <div class={corner_class}></div>
                <div class="app">
                    <aside class={classes!("sidebar", if *mobile_menu_open { Some("mobile-open") } else { None })}>
                        <div class="brand">
                            <h1>{"NipLock"}</h1>
                        </div>
                        <button class={classes!("nav-item", if *page == Page::Vault { Some("active") } else { None })} onclick={on_nav_vault}>{"Vault"}</button>
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
                            <input class="search" placeholder="Search vault..." value={(*search).clone()} oninput={on_search} />
                            <div class="top-right">
                                <button class="btn" onclick={on_sync_now.clone()} disabled={!*unlocked}>{"Refresh"}</button>
                                <button class="unlock" onclick={on_toggle_unlock_panel}>{ if *unlocked { "Lock" } else { "Unlock" } }</button>
                            </div>
                            if *unlock_panel_open {
                                <div class="unlock-panel">
                                    <div style="font-weight:700; margin-bottom:8px;">{"Unlock Vault Sync"}</div>
                                    <div class="row" style="margin-bottom:8px;">
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Nsec { Some("primary") } else { None })} onclick={on_unlock_method_nsec}>{"NSEC"}</button>
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Amber { Some("primary") } else { None })} onclick={on_unlock_method_amber}>{"Amber / Bunker"}</button>
                                        <button class={classes!("btn", if *unlock_method == UnlockMethod::Nip07 { Some("primary") } else { None })} onclick={on_unlock_method_nip07}>{"nos2xfox"}</button>
                                    </div>
                                    if *unlock_method == UnlockMethod::Nip07 {
                                        <div class="muted" style="margin-top: 8px; font-size: 0.85rem;">{"Using browser signer via NIP-07 (nos2xfox)."}</div>
                                    } else {
                                        <input
                                            class="input"
                                            type={if *unlock_method == UnlockMethod::Nsec { "password" } else { "text" }}
                                            placeholder={if *unlock_method == UnlockMethod::Nsec { "nsec1..." } else { "bunker://... or nostrconnect://..." }}
                                            value={(*unlock_input).clone()}
                                            oninput={on_unlock_input.clone()}
                                        />
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
                                    Page::SecurityAudit => render_audit_page(&entries, weak_count, health_score),
                                    Page::Settings => render_settings_page(
                                        (*signer_credential).clone(),
                                        (*active_npub).clone(),
                                        *unlocked,
                                        sync_label,
                                        last_sync.as_ref().cloned(),
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
        health_score: f64,
        last_sync: Option<String>,
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
                        <button class="btn" onclick={on_back}>{"← Back to Vault"}</button>
                        <span>{format!("Vault / {}", entry.service)}</span>
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
                                    <span class="strength"><i style={format!("width:{}%; background-color:{}", strength_width_pct(bits), strength_color(bits))}></i></span>
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
                                    <input class="input" placeholder="Service" value={draft.service.clone()} oninput={on_draft_service}/>
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
                    <div class="explorer-head">
                        <div>
                            <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"Vault Explorer"}</h2>
                        </div>
                        <div class="stats">
                            <div class="stat">
                                <div class="k">{"Health Score"}</div>
                                <div class="v" style="color: var(--teal);">{format!("{:.1}%", health_score)}</div>
                            </div>
                            <div class="stat">
                                <div class="k">{"Last Sync"}</div>
                                <div class="v" style="font-size:1.3rem;">{
                                    last_sync
                                        .map(|ts| format_human_timestamp(&ts))
                                        .unwrap_or_else(|| "Never".to_string())
                                }</div>
                            </div>
                        </div>
                    </div>

                    <div class="section">
                        if !unlocked {
                            <div class="muted" style="margin-bottom:10px;">{"Unlock to load your vault entries."}</div>
                        }
                        <table class="table">
                            <thead>
                                <tr>
                                    <th>{"Service"}</th>
                                    <th>{"Username"}</th>
                                    <th>{"Strength"}</th>
                                    <th>{"Last Modified"}</th>
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
                                        <td>
                                            <div class="row">
                                                <span class="strength"><i style={format!("width:{}%; background-color:{}", strength_width_pct(bits), strength_color(bits))}></i></span>
                                                <span class="muted strength-text">{strength_label(bits)}</span>
                                            </div>
                                        </td>
                                        <td class="copy-cell" ondblclick={on_copy_secret}>{entry.updated_at.format("%b %d, %Y").to_string()}</td>
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
                                <span class="strength"><i style={format!("width:{}%; background-color:{}", strength_width_pct(generated_bits), strength_color(generated_bits))}></i></span>
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
                    <input class="input" placeholder="Service" value={draft.service.clone()} oninput={on_draft_service}/>
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
                        <span class="strength"><i style={format!("width:{}%; background-color:{}", strength_width_pct(draft_bits), strength_color(draft_bits))}></i></span>
                        <span class="muted strength-text">{format!("Entropy: {draft_bits:.1} bits ({})", strength_label(draft_bits))}</span>
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

    fn render_audit_page(entries: &[PasswordEntry], weak_count: usize, health_score: f64) -> Html {
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
                                <th>{"Service"}</th>
                                <th>{"Username"}</th>
                                <th>{"Entropy"}</th>
                                <th>{"Action"}</th>
                            </tr>
                        </thead>
                        <tbody>
                        {for ranked.iter().take(12).map(|entry| {
                            let bits = entropy_bits(&entry.secret);
                            html! {
                                <tr>
                                    <td>{entry.service.clone()}</td>
                                    <td>{entry.username.clone()}</td>
                                    <td>{format!("{bits:.1} bits")}</td>
                                    <td>
                                        <button class="btn">{"Rotate"}</button>
                                    </td>
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
        last_sync: Option<String>,
    ) -> Html {
        html! {
            <>
                <h2 style="margin:0; font-size:2.2rem; font-family:'Space Grotesk', 'Segoe UI', sans-serif;">{"System Preferences"}</h2>

                <div class="section">
                    <div class="detail-label">{"Sync State"}</div>
                    <div style="font-size:1.25rem; font-weight:700; color:var(--teal);">{if unlocked { "Unlocked" } else { "Locked" }}</div>
                    <div class="muted">{sync_label}</div>
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
                        <div class="muted" style="margin-top:8px;">{format!("Last sync: {}", format_human_timestamp(&ts))}</div>
                    }
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

    fn strength_color(bits: f64) -> &'static str {
        if bits >= 110.0 {
            "#39d49e"
        } else if bits >= 85.0 {
            "#d8d64f"
        } else if bits >= 60.0 {
            "#f0a35f"
        } else {
            "#ef6e6e"
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

    fn spawn_sync(
        signer_credential: String,
        local_entries: Vec<PasswordEntry>,
        entries_state: UseStateHandle<Vec<PasswordEntry>>,
        sync_state: UseStateHandle<SyncState>,
        last_sync: UseStateHandle<Option<String>>,
        sync_in_flight: UseStateHandle<bool>,
        live_sync: Rc<RefCell<Option<NostrSync>>>,
    ) {
        if *sync_in_flight {
            return;
        }

        sync_in_flight.set(true);
        sync_state.set(SyncState::Syncing);

        spawn_local(async move {
            if signer_credential.trim().is_empty() {
                sync_state.set(SyncState::Error(
                    "set signer credential to enable NIP-17 sync".to_string(),
                ));
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
                        sync_in_flight.set(false);
                        return;
                    }
                };

                match NostrSync::new_with_signer(
                    signer,
                    DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect(),
                )
                .await
                {
                    Ok(v) => {
                        *live_sync.borrow_mut() = Some(v.clone());
                        v
                    }
                    Err(err) => {
                        sync_state.set(SyncState::Error(format!("relay connect failed: {err}")));
                        sync_in_flight.set(false);
                        return;
                    }
                }
            };

            let local = to_map(&local_entries);
            match sync.sync(&local).await {
                Ok((merged, _summary)) => {
                    let next = from_map(merged);
                    save_entries(&next);
                    entries_state.set(next);
                    last_sync.set(Some(Utc::now().to_rfc3339()));
                    sync_state.set(SyncState::Idle);
                }
                Err(err) => {
                    *live_sync.borrow_mut() = None;
                    sync_state.set(SyncState::Error(format!("sync failed: {err}")));
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
                let _ = storage.set_item(STORAGE_KEY, &payload);
            }
        }
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
