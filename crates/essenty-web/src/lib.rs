//! Web/WASM adapter for the Essenty Rust runtime.
//!
//! Bootstrap scope: host-compilable, pure Rust mapping logic plus narrow
//! `wasm32`-only bindings. Browser APIs (`wasm-bindgen`, `js-sys`,
//! `web-sys`) are used exclusively inside this crate, are gated behind
//! `cfg(target_arch = "wasm32")` dependencies, and never leak into the
//! core crates.
//!
//! Integration surfaces:
//!
//! - Page Visibility API → lifecycle ([`VisibilityLifecycle`])
//! - History API / `popstate` → back handling ([`HistoryBackBridge`])
//! - `sessionStorage` / `localStorage` → state persistence ([`StorageKey`])
//!
//! On `wasm32`, `wasm::BrowserLifecycle` installs live lifecycle listeners,
//! `wasm::BrowserHistoryBack` observes `popstate`, and
//! `wasm::BrowserStorage` persists opaque state bytes. Mapping logic remains
//! host-testable.
//!
//! # Example
//!
//! ```rust
//! use essenty_web::VisibilityLifecycle;
//!
//! let host = VisibilityLifecycle::new();
//! host.on_visibility_str("visible");
//! assert!(host.registry().state().is_resumed());
//! ```

mod history;
mod lifecycle;
mod storage;

pub use history::HistoryBackBridge;
pub use lifecycle::{PageVisibility, VisibilityLifecycle};
pub use storage::{StorageArea, StorageKey};

#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[cfg(test)]
mod tests;
