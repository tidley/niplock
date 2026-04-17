#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("passwd-web is a wasm binary. Build with: trunk serve --bin passwd-web");
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::collections::HashMap;

    use chrono::Utc;
    use gloo_events::EventListener;
    use nostr_sdk::prelude::Keys;
    use passwd::model::PasswordEntry;
    use passwd::nostr_sync::NostrSync;
    use uuid::Uuid;
    use wasm_bindgen_futures::{spawn_local, JsFuture};
    use web_sys::{window, HtmlInputElement, HtmlTextAreaElement};
    use yew::prelude::*;

    const STORAGE_KEY: &str = "passwd.vault.v1";
    const DEFAULT_RELAYS: [&str; 2] = ["wss://nip17.tomdwyer.uk", "wss://nip17.com"];

    const CSS: &str = r#"
:root {
  --bg: #f4f7fb;
  --panel: #ffffff;
  --text: #1f2937;
  --muted: #6b7280;
  --line: #d1d5db;
  --accent: #16a34a;
  --warn: #f59e0b;
  --err: #dc2626;
  --primary: #2563eb;
}
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
}
.app {
  max-width: 1200px;
  margin: 0 auto;
  padding: 16px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}
.title {
  font-family: "Space Grotesk", "Segoe UI", sans-serif;
  font-size: 1.35rem;
  margin: 0;
}
.muted {
  color: var(--muted);
  font-size: 0.9rem;
}
.card {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 12px;
  margin-bottom: 10px;
}
label {
  display: block;
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--muted);
  margin-bottom: 0.3rem;
}
input, textarea {
  width: 100%;
  box-sizing: border-box;
  background: #fff;
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px 10px;
  margin-bottom: 10px;
}
textarea {
  min-height: 66px;
  resize: vertical;
}
button {
  background: var(--primary);
  color: #fff;
  border: 1px solid var(--primary);
  border-radius: 8px;
  padding: 7px 10px;
  cursor: pointer;
  font-weight: 600;
  font-size: 0.85rem;
}
button.alt {
  background: #fff;
  color: #111827;
  border-color: var(--line);
}
button.danger {
  background: #fff;
  color: var(--err);
  border-color: #fecaca;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.toolbar {
  display: grid;
  gap: 8px;
  grid-template-columns: 1.5fr 1fr 200px auto;
  align-items: end;
}
.layout {
  display: grid;
  gap: 10px;
  grid-template-columns: 320px 1fr;
}
.field-inline {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 8px;
  align-items: end;
}
.icon-btn {
  min-width: 40px;
}
.table-wrap {
  overflow: auto;
}
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.9rem;
}
th, td {
  text-align: left;
  border-bottom: 1px solid var(--line);
  padding: 8px;
  white-space: nowrap;
}
th {
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--muted);
}
tr:hover {
  background: #f9fafb;
}
.copy-cell {
  cursor: copy;
}
.copy-hint {
  font-size: 0.8rem;
  color: var(--muted);
}
.corner {
  position: fixed;
  top: 10px;
  right: 10px;
  width: 11px;
  height: 11px;
  border-radius: 999px;
  border: 1px solid #fff;
}
.corner.idle { background: var(--accent); }
.corner.syncing { background: var(--warn); animation: pulse 1s infinite; }
.corner.error { background: var(--err); }
@keyframes pulse {
  0% { opacity: 0.5; transform: scale(0.9); }
  50% { opacity: 1; transform: scale(1.06); }
  100% { opacity: 0.5; transform: scale(0.9); }
}
@media (max-width: 640px) {
  .app { padding: 10px; }
}
@media (max-width: 1000px) {
  .layout {
    grid-template-columns: 1fr;
  }
  .toolbar {
    grid-template-columns: 1fr;
  }
}
"#;

    #[derive(Clone, PartialEq)]
    enum SyncState {
        Idle,
        Syncing,
        Error(String),
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
        let entries = use_state(load_entries);
        let draft = use_state(Draft::default);
        let nsec = use_state(String::new);
        let sync_state = use_state(|| SyncState::Idle);
        let last_sync = use_state(|| None::<String>);
        let sync_in_flight = use_state(|| false);
        let show_secret = use_state(|| false);
        let search = use_state(String::new);
        let copy_notice = use_state(|| None::<String>);

        {
            let entries = entries.clone();
            let nsec = nsec.clone();
            let sync_state = sync_state.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();

            use_effect_with((), move |_| {
                spawn_sync(
                    nsec.clone(),
                    entries.clone(),
                    sync_state.clone(),
                    last_sync.clone(),
                    sync_in_flight.clone(),
                );

                let doc_listener = window().and_then(|w| w.document()).map(|doc| {
                    let entries = entries.clone();
                    let nsec = nsec.clone();
                    let sync_state = sync_state.clone();
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    EventListener::new(&doc, "visibilitychange", move |_| {
                        if let Some(document) = window().and_then(|w| w.document()) {
                            if document.hidden() {
                                spawn_sync(
                                    nsec.clone(),
                                    entries.clone(),
                                    sync_state.clone(),
                                    last_sync.clone(),
                                    sync_in_flight.clone(),
                                );
                            }
                        }
                    })
                });

                let pagehide_listener = window().map(|win| {
                    let entries = entries.clone();
                    let nsec = nsec.clone();
                    let sync_state = sync_state.clone();
                    let last_sync = last_sync.clone();
                    let sync_in_flight = sync_in_flight.clone();
                    EventListener::new(&win, "pagehide", move |_| {
                        spawn_sync(
                            nsec.clone(),
                            entries.clone(),
                            sync_state.clone(),
                            last_sync.clone(),
                            sync_in_flight.clone(),
                        );
                    })
                });

                move || {
                    drop(doc_listener);
                    drop(pagehide_listener);
                }
            });
        }

        let on_nsec_input = {
            let nsec = nsec.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlInputElement = event.target_unchecked_into();
                nsec.set(input.value());
            })
        };

        let on_service_input = {
            let draft = draft.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlInputElement = event.target_unchecked_into();
                let mut next = (*draft).clone();
                next.service = input.value();
                draft.set(next);
            })
        };

        let on_username_input = {
            let draft = draft.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlInputElement = event.target_unchecked_into();
                let mut next = (*draft).clone();
                next.username = input.value();
                draft.set(next);
            })
        };

        let on_secret_input = {
            let draft = draft.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlInputElement = event.target_unchecked_into();
                let mut next = (*draft).clone();
                next.secret = input.value();
                draft.set(next);
            })
        };

        let on_notes_input = {
            let draft = draft.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlTextAreaElement = event.target_unchecked_into();
                let mut next = (*draft).clone();
                next.notes = input.value();
                draft.set(next);
            })
        };

        let on_save_entry = {
            let entries = entries.clone();
            let draft = draft.clone();
            let show_secret = show_secret.clone();
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

                let last_event_id = map.get(&id).and_then(|existing| existing.last_event_id.clone());

                map.insert(
                    id.clone(),
                    PasswordEntry {
                        id,
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
                entries.set(next);
                draft.set(Draft::default());
                show_secret.set(false);
            })
        };

        let on_reset_form = {
            let draft = draft.clone();
            let show_secret = show_secret.clone();
            Callback::from(move |_| {
                draft.set(Draft::default());
                show_secret.set(false);
            })
        };

        let on_sync_now = {
            let entries = entries.clone();
            let nsec = nsec.clone();
            let sync_state = sync_state.clone();
            let last_sync = last_sync.clone();
            let sync_in_flight = sync_in_flight.clone();
            Callback::from(move |_| {
                spawn_sync(
                    nsec.clone(),
                    entries.clone(),
                    sync_state.clone(),
                    last_sync.clone(),
                    sync_in_flight.clone(),
                );
            })
        };

        let on_toggle_secret = {
            let show_secret = show_secret.clone();
            Callback::from(move |_| {
                show_secret.set(!*show_secret);
            })
        };

        let on_search_input = {
            let search = search.clone();
            Callback::from(move |event: InputEvent| {
                let input: HtmlInputElement = event.target_unchecked_into();
                search.set(input.value());
            })
        };

        let filtered_entries: Vec<PasswordEntry> = {
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
        };

        let corner_class = match &*sync_state {
            SyncState::Idle => "corner idle",
            SyncState::Syncing => "corner syncing",
            SyncState::Error(_) => "corner error",
        };

        let sync_label = match &*sync_state {
            SyncState::Idle => "Idle".to_string(),
            SyncState::Syncing => "Syncing".to_string(),
            SyncState::Error(err) => format!("Sync error: {err}"),
        };

        html! {
            <>
                <style>{CSS}</style>
                <div class={corner_class}></div>
                <div class="app">
                    <div class="head">
                        <h1 class="title">{"passwd"}</h1>
                        <div class="muted">{sync_label.clone()}</div>
                    </div>

                    <section class="card toolbar">
                        <div>
                            <label for="search">{"Search"}</label>
                            <input id="search" placeholder="Search service / user / notes" value={(*search).clone()} oninput={on_search_input} />
                        </div>
                        <div>
                            <label for="nsec">{"Nostr nsec"}</label>
                            <input id="nsec" type="password" placeholder="nsec1..." value={(*nsec).clone()} oninput={on_nsec_input} />
                        </div>
                        <div class="muted">
                            {format!("Entries: {}", filtered_entries.len())}
                            if let Some(ts) = &*last_sync {
                                <div>{format!("Last sync: {ts}")}</div>
                            }
                            if let Some(msg) = &*copy_notice {
                                <div>{msg.clone()}</div>
                            }
                        </div>
                        <div class="row">
                            <button onclick={on_sync_now}>{"Sync now"}</button>
                        </div>
                    </section>

                    <div class="layout">
                        <section class="card">
                            <h3>{ if draft.id.is_some() { "Edit entry" } else { "Add entry" } }</h3>
                            <label for="service">{"Service"}</label>
                            <input id="service" value={draft.service.clone()} oninput={on_service_input} />
                            <label for="username">{"Username"}</label>
                            <input id="username" value={draft.username.clone()} oninput={on_username_input} />
                            <label for="secret">{"Password / Secret"}</label>
                            <div class="field-inline">
                                <input id="secret" type={if *show_secret { "text" } else { "password" }} value={draft.secret.clone()} oninput={on_secret_input} />
                                <button class="alt icon-btn" type="button" onclick={on_toggle_secret}>{"👁"}</button>
                            </div>
                            <label for="notes">{"Notes"}</label>
                            <textarea id="notes" value={draft.notes.clone()} oninput={on_notes_input}></textarea>
                            <div class="row">
                                <button onclick={on_save_entry}>{"Save entry"}</button>
                                <button class="alt" onclick={on_reset_form}>{"Clear form"}</button>
                            </div>
                        </section>

                        <section class="card">
                        <div class="row" style="justify-content: space-between;">
                            <h3>{format!("Vault entries ({})", filtered_entries.len())}</h3>
                            <div class="copy-hint">{"Double-click username or password to copy"}</div>
                        </div>
                        {
                            if filtered_entries.is_empty() {
                                html! { <p class="muted">{"No entries yet."}</p> }
                            } else {
                                html! {
                                    <div class="table-wrap">
                                    <table>
                                        <thead>
                                            <tr>
                                                <th>{"Service"}</th>
                                                <th>{"Username"}</th>
                                                <th>{"Password"}</th>
                                                <th>{"Updated"}</th>
                                                <th>{"Actions"}</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                    {for filtered_entries.iter().map(|entry| {
                                        let id = entry.id.clone();
                                        let entry_for_edit = entry.clone();
                                        let entry_for_copy_user = entry.clone();
                                        let entry_for_copy_secret = entry.clone();
                                        let entries_for_delete = entries.clone();
                                        let entries_for_edit = entries.clone();
                                        let draft_for_edit = draft.clone();
                                        let copy_notice_for_user = copy_notice.clone();
                                        let copy_notice_for_secret = copy_notice.clone();
                                        let show_secret_for_edit = show_secret.clone();

                                        let on_delete = Callback::from(move |_| {
                                            let mut map = to_map(&entries_for_delete);
                                            map.remove(&id);
                                            let next = from_map(map);
                                            save_entries(&next);
                                            entries_for_delete.set(next);
                                        });

                                        let on_edit = Callback::from(move |_| {
                                            draft_for_edit.set(Draft {
                                                id: Some(entry_for_edit.id.clone()),
                                                service: entry_for_edit.service.clone(),
                                                username: entry_for_edit.username.clone(),
                                                secret: entry_for_edit.secret.clone(),
                                                notes: entry_for_edit.notes.clone().unwrap_or_default(),
                                            });
                                            show_secret_for_edit.set(false);
                                            entries_for_edit.set((*entries_for_edit).clone());
                                        });

                                        let on_copy_user = Callback::from(move |_| {
                                            copy_to_clipboard(
                                                entry_for_copy_user.username.clone(),
                                                copy_notice_for_user.clone(),
                                                format!("Copied username for {}", entry_for_copy_user.service),
                                            );
                                        });

                                        let on_copy_secret = Callback::from(move |_| {
                                            copy_to_clipboard(
                                                entry_for_copy_secret.secret.clone(),
                                                copy_notice_for_secret.clone(),
                                                format!("Copied password for {}", entry_for_copy_secret.service),
                                            );
                                        });

                                        html! {
                                            <tr key={entry.id.clone()}>
                                                <td>{&entry.service}</td>
                                                <td class="copy-cell" ondblclick={on_copy_user}>{&entry.username}</td>
                                                <td class="copy-cell" ondblclick={on_copy_secret}>{"••••••••"}</td>
                                                <td>{entry.updated_at.format("%Y-%m-%d %H:%M").to_string()}</td>
                                                <td>
                                                    <div class="row">
                                                        <button class="alt" onclick={on_edit}>{"Edit"}</button>
                                                        <button class="danger" onclick={on_delete}>{"Delete"}</button>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                    })}
                                        </tbody>
                                    </table>
                                    </div>
                                }
                            }
                        }
                    </section>
                    </div>
                </div>
            </>
        }
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

    fn spawn_sync(
        nsec: UseStateHandle<String>,
        entries: UseStateHandle<Vec<PasswordEntry>>,
        sync_state: UseStateHandle<SyncState>,
        last_sync: UseStateHandle<Option<String>>,
        sync_in_flight: UseStateHandle<bool>,
    ) {
        if *sync_in_flight {
            return;
        }

        sync_in_flight.set(true);
        sync_state.set(SyncState::Syncing);

        spawn_local(async move {
            if nsec.trim().is_empty() {
                sync_state.set(SyncState::Error(
                    "set nsec to enable NIP-17 sync".to_string(),
                ));
                sync_in_flight.set(false);
                return;
            }

            let keys = match Keys::parse(nsec.trim()) {
                Ok(v) => v,
                Err(err) => {
                    sync_state.set(SyncState::Error(format!("invalid nsec: {err}")));
                    sync_in_flight.set(false);
                    return;
                }
            };

            let sync = match NostrSync::new(
                keys,
                DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect(),
            )
            .await
            {
                Ok(v) => v,
                Err(err) => {
                    sync_state.set(SyncState::Error(format!("relay connect failed: {err}")));
                    sync_in_flight.set(false);
                    return;
                }
            };

            let local = to_map(&entries);
            match sync.sync(&local).await {
                Ok((merged, _summary)) => {
                    let next = from_map(merged);
                    save_entries(&next);
                    entries.set(next);
                    last_sync.set(Some(Utc::now().to_rfc3339()));
                    sync_state.set(SyncState::Idle);
                }
                Err(err) => {
                    sync_state.set(SyncState::Error(format!("sync failed: {err}")));
                }
            }

            sync.shutdown().await;
            sync_in_flight.set(false);
        });
    }

    fn load_entries() -> Vec<PasswordEntry> {
        let Some(storage) = local_storage() else {
            return vec![];
        };

        let Ok(Some(raw)) = storage.get_item(STORAGE_KEY) else {
            return vec![];
        };

        match serde_json::from_str::<Vec<PasswordEntry>>(&raw) {
            Ok(entries) => entries,
            Err(_) => vec![],
        }
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
                .then(a.username.to_ascii_lowercase().cmp(&b.username.to_ascii_lowercase()))
        });
        entries
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::run();
}
