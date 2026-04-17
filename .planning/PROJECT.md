# passwd

Rust-first, multi-platform password manager that uses Nostr NIP-17 private messages (DM-to-self) as the remote replication layer.

## Core intent
- Local-first password vault with encrypted-at-rest local data (pending implementation).
- Nostr NIP-17 self-DMs are the source of remote replication and restore.
- Startup and shutdown background sync should keep local and relay state aligned.
- If local entries are missing remotely, app should re-broadcast to configured NIP-17 relays.

## Initial relay set
- `wss://nip17.tomdwyer.uk`
- `wss://nip17.com`

## Current baseline
- Rust project scaffolded.
- Nostr sync module created.
- Local snapshot store created.
- Startup/shutdown sync orchestration created.

## Platform target
- web, macOS, Linux, Android, iOS, Windows

Current code establishes portable Rust core that can be reused in platform-specific shells.
