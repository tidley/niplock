# Repo Instructions

## Native run
- Set `PASSWD_NSEC` to your Nostr private key (`nsec...` or hex) before starting.
- Run with `cargo run`.

## Web GUI run (WASM)
1. Install Trunk if needed: `cargo install trunk`.
2. Add wasm target if needed: `rustup target add wasm32-unknown-unknown`.
3. Start web UI: `trunk serve --bin passwd-web --open`.

## Sync behavior
- Startup sync runs in background and updates the sync indicator state.
- Graceful shutdown (`Ctrl+C`) performs a final sync before process exit.
- Web app sync triggers on startup, manual sync, tab hide, and pagehide.
- Sync reads NIP-17 gift-wrapped events addressed to your pubkey.
- Local entries missing remotely are re-published.

## Safety notes
- Current local store uses plain JSON for bootstrap only.
- Native store path uses local filesystem.
- Web store uses browser LocalStorage.
- Do not use for production until local encryption-at-rest is added.
