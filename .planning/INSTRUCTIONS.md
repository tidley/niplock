# Repo Instructions

## Run
- Set `PASSWD_NSEC` to your Nostr private key (`nsec...` or hex) before starting.
- Run with `cargo run`.

## Sync behavior
- Startup sync runs in background and updates the sync indicator state.
- Graceful shutdown (`Ctrl+C`) performs a final sync before process exit.
- Sync reads NIP-17 gift-wrapped events addressed to your pubkey.
- Local entries missing remotely are re-published.

## Safety notes
- Current local store uses plain JSON for bootstrap only.
- Do not use for production until local encryption-at-rest is added.
