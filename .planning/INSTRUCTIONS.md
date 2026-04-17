# Repo Instructions

## Native run
- Set one of these before starting:
  - `PASSWD_SIGNER` with `nsec...`, hex key, or `bunker://...` / `nostrconnect://...`
  - or `PASSWD_NSEC` (legacy fallback)
- Run with `cargo run`.

## Web GUI run (WASM)
1. Install Trunk if needed: `cargo install trunk`.
2. Add wasm target if needed: `rustup target add wasm32-unknown-unknown`.
3. Start web UI: `trunk serve --open`.

## Deployment (no GitHub)
Use static build + SSH/rsync deploy to your own server (works with ngit-hosted repos).

### Build only
- `./scripts/build-web.sh`

### Build + deploy via SSH
1. Set environment variables:
   - `DEPLOY_HOST` (example `nsyte.run` or server IP)
   - `DEPLOY_USER` (example `deploy`)
   - `DEPLOY_PATH` (example `/var/www/nsyte.run`)
   - optional `DEPLOY_PORT` (default `22`)
2. Run:
   - `./scripts/deploy-web-ssh.sh`

## Nginx setup for nsyte.run
- Use config template: `deploy/nginx/nsyte.run.conf`
- It serves static files from `/var/www/nsyte.run`
- Includes SPA fallback (`try_files ... /index.html`)

## Sync behavior
- Startup sync runs in background and updates the sync indicator state.
- Graceful shutdown (`Ctrl+C`) performs a final sync before process exit.
- Web app sync triggers on startup, manual sync, tab hide, pagehide, and when entries are modified.
- Sync reads NIP-17 gift-wrapped events addressed to your pubkey.
- Local entries missing remotely are re-published.
- Signer support includes local keys and NIP-46 remote signer/bunker URIs (compatible with signer apps like nos2x and Amber when they provide `bunker://` URIs).

## Safety notes
- Current local store uses plain JSON for bootstrap only.
- Native store path uses local filesystem.
- Web store uses browser LocalStorage.
- Do not use for production until local encryption-at-rest is added.
