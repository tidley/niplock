# ROADMAP

## Phase 1
- Scaffold Rust core app and local vault snapshot storage.
- Implement baseline Nostr sync for NIP-17 self-DM replication.
- Implement startup/shutdown sync orchestration with indicator state.

## Phase 2
- Add encryption-at-rest for local vault.
- Build real UI shell with corner sync indicator (desktop + web).
- Add CRUD interface for password entries.

## Phase 3
- Add mobile shells (Android/iOS) using shared Rust core.
- Add background sync scheduling and conflict-resolution UI.
- Add integration tests against NIP-17 relays.
