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
    use wasm_bindgen_futures::spawn_local;
    use web_sys::{window, HtmlInputElement};
    use yew::prelude::*;

    const STORAGE_KEY: &str = "passwd.vault.v1";
    const DEFAULT_RELAYS: [&str; 2] = ["wss://nip17.tomdwyer.uk", "wss://nip17.com"];

    const CSS: &str = r#"
:root {
  --bg: #0f172a;
  --panel: #111c37;
  --panel-2: #172447;
  --text: #dbeafe;
  --muted: #93a6c6;
  --accent: #22c55e;
  --warn: #f97316;
  --err: #ef4444;
  --line: #27406f;
}
body {
  margin: 0;
  background: radial-gradient(1200px 500px at 20% -20%, #1f3b77, transparent), var(--bg);
  color: var(--text);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
}
.app {
  max-width: 980px;
  margin: 0 auto;
  padding: 1.25rem;
}
.head {
  margin-bottom: 1rem;
}
.title {
  font-family: "Space Grotesk", "Segoe UI", sans-serif;
  letter-spacing: 0.02em;
  margin: 0;
}
.muted {
  color: var(--muted);
}
.grid {
  display: grid;
  gap: 1rem;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
}
.card {
  background: linear-gradient(180deg, var(--panel), var(--panel-2));
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 1rem;
}
label {
  display: block;
  font-size: 0.85rem;
  color: var(--muted);
  margin-bottom: 0.3rem;
}
input, textarea {
  width: 100%;
  box-sizing: border-box;
  background: #0e1630;
  color: var(--text);
  border: 1px solid #223d6e;
  border-radius: 10px;
  padding: 0.65rem;
  margin-bottom: 0.75rem;
}
textarea {
  min-height: 82px;
  resize: vertical;
}
button {
  background: #1d4ed8;
  color: #eff6ff;
  border: 0;
  border-radius: 10px;
  padding: 0.55rem 0.8rem;
  cursor: pointer;
  font-weight: 600;
}
button.alt {
  background: #334155;
}
button.danger {
  background: #991b1b;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
}
.entry {
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 0.7rem;
  margin-bottom: 0.6rem;
  background: #0e1630;
}
.entry h4 {
  margin: 0 0 0.3rem 0;
}
.corner {
  position: fixed;
  top: 12px;
  right: 12px;
  width: 11px;
  height: 11px;
  border-radius: 999px;
  box-shadow: 0 0 0 2px rgba(15, 23, 42, 0.8);
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
  .app { padding: 0.8rem; }
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
                let input: HtmlInputElement = event.target_unchecked_into();
                let mut next = (*draft).clone();
                next.notes = input.value();
                draft.set(next);
            })
        };

        let on_save_entry = {
            let entries = entries.clone();
            let draft = draft.clone();
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
            })
        };

        let on_reset_form = {
            let draft = draft.clone();
            Callback::from(move |_| {
                draft.set(Draft::default());
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
                        <h1 class="title">{"passwd (web)"}</h1>
                        <div class="muted">{"Web-first NIP-17 password vault sync"}</div>
                    </div>
                    <div class="grid">
                        <section class="card">
                            <label for="nsec">{"Nostr nsec"}</label>
                            <input id="nsec" type="password" placeholder="nsec1..." value={(*nsec).clone()} oninput={on_nsec_input} />
                            <div class="row">
                                <button onclick={on_sync_now}>{"Sync now"}</button>
                                <span class="muted">{sync_label}</span>
                            </div>
                            if let Some(ts) = &*last_sync {
                                <p class="muted">{format!("Last sync: {ts}")}</p>
                            }
                        </section>

                        <section class="card">
                            <h3>{ if draft.id.is_some() { "Edit entry" } else { "Add entry" } }</h3>
                            <label for="service">{"Service"}</label>
                            <input id="service" value={draft.service.clone()} oninput={on_service_input} />
                            <label for="username">{"Username"}</label>
                            <input id="username" value={draft.username.clone()} oninput={on_username_input} />
                            <label for="secret">{"Password / Secret"}</label>
                            <input id="secret" type="password" value={draft.secret.clone()} oninput={on_secret_input} />
                            <label for="notes">{"Notes"}</label>
                            <textarea id="notes" value={draft.notes.clone()} oninput={on_notes_input}></textarea>
                            <div class="row">
                                <button onclick={on_save_entry}>{"Save entry"}</button>
                                <button class="alt" onclick={on_reset_form}>{"Clear form"}</button>
                            </div>
                        </section>
                    </div>

                    <section class="card" style="margin-top: 1rem;">
                        <h3>{format!("Vault entries ({})", entries.len())}</h3>
                        {
                            if entries.is_empty() {
                                html! { <p class="muted">{"No entries yet."}</p> }
                            } else {
                                html! {
                                    {for entries.iter().map(|entry| {
                                        let id = entry.id.clone();
                                        let entry_for_edit = entry.clone();
                                        let entries_for_delete = entries.clone();
                                        let entries_for_edit = entries.clone();
                                        let draft_for_edit = draft.clone();

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
                                            entries_for_edit.set((*entries_for_edit).clone());
                                        });

                                        html! {
                                            <article class="entry" key={entry.id.clone()}>
                                                <h4>{&entry.service}</h4>
                                                <div class="muted">{format!("{}  |  {}", entry.username, entry.updated_at)}</div>
                                                <div>{"Secret: "}<strong>{"••••••••"}</strong></div>
                                                if let Some(notes) = &entry.notes {
                                                    <div class="muted">{notes.clone()}</div>
                                                }
                                                <div class="row" style="margin-top: 0.45rem;">
                                                    <button class="alt" onclick={on_edit}>{"Edit"}</button>
                                                    <button class="danger" onclick={on_delete}>{"Delete"}</button>
                                                </div>
                                            </article>
                                        }
                                    })}
                                }
                            }
                        }
                    </section>
                </div>
            </>
        }
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
