use std::sync::atomic::{AtomicU8, Ordering};

const STATE_IDLE: u8 = 0;
const STATE_SYNCING: u8 = 1;
const STATE_ERROR: u8 = 2;

#[derive(Debug, Default)]
pub struct SyncIndicator {
    state: AtomicU8,
}

impl SyncIndicator {
    pub fn set_syncing(&self) {
        self.state.store(STATE_SYNCING, Ordering::Relaxed);
    }

    pub fn set_idle(&self) {
        self.state.store(STATE_IDLE, Ordering::Relaxed);
    }

    pub fn set_error(&self) {
        self.state.store(STATE_ERROR, Ordering::Relaxed);
    }

    pub fn render_hint(&self) -> &'static str {
        match self.state.load(Ordering::Relaxed) {
            STATE_SYNCING => "[corner:syncing]",
            STATE_ERROR => "[corner:sync-error]",
            _ => "[corner:idle]",
        }
    }
}
