//! Live browser adapters. Each binding owns and removes its DOM listeners.

mod browser_history;
mod browser_lifecycle;
mod browser_storage;

pub use browser_history::BrowserHistoryBack;
pub use browser_lifecycle::{
    BrowserError, BrowserLifecycle, as_pop_state, current_visibility_state, has_window,
};
pub use browser_storage::{BrowserStorage, BrowserStorageError};
