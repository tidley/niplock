# REQUIREMENTS

## Functional
1. App stores password entries locally and syncs them via NIP-17 DMs to self.
2. App syncs on startup and before shutdown.
3. App merges remote entries for multi-device usage.
4. App re-broadcasts local entries missing on relays.
5. Default relay list includes `wss://nip17.tomdwyer.uk` and `wss://nip17.com`.

## Non-functional
1. Keep sync non-blocking from UI perspective (background task + subtle state indicator).
2. Keep core logic in Rust and portable across all target platforms.
3. Preserve schema-versioned event payloads for future migrations.

## Security
1. Do not log plaintext secrets.
2. Use Nostr encrypted private messages (NIP-17/NIP-59 flow).
3. Add local encryption-at-rest before production release.
