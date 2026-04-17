#[cfg(not(target_arch = "wasm32"))]
pub mod app;
pub mod model;
pub mod nostr_sync;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
#[cfg(not(target_arch = "wasm32"))]
pub mod ui;
