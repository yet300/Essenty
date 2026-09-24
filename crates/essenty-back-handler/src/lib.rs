//! Pure Rust back-event dispatcher.
//!
//! Handles both ordinary back presses and the generalized predictive
//! (progress-based) back gesture model:
//!
//! - callbacks register with a priority and an enabled flag
//! - the highest-priority enabled handler wins; ties break toward the most
//!   recently registered callback
//! - [`BackDispatcher::back`] performs a regular back invocation
//! - [`BackDispatcher::predictive_start`] claims the gesture for the current
//!   winner; [`BackDispatcher::predictive_progress`],
//!   [`BackDispatcher::predictive_cancel`], and
//!   [`BackDispatcher::predictive_invoke`] route to that claimed handler so
//!   the gesture stays coherent even if priorities change mid-gesture;
//!   removal cancels the owner and lets a later progress event claim a fallback
//!
//! No Android (or any platform) API appears here; platform crates forward
//! native back events into this dispatcher.
//!
//! The dispatcher is single-threaded (`!Send`, `!Sync`) and synchronous. It
//! currently requires `std`, but only `alloc`/`core` containers are used, so
//! future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_back_handler::BackDispatcher;
//!
//! let mut dispatcher = BackDispatcher::new();
//! let handle = dispatcher.register(0, true, |event| {
//!     println!("back: {event:?}");
//! });
//! assert!(dispatcher.back());
//! dispatcher.unregister(handle.id());
//! ```

mod dispatcher;
mod error;
mod event;

pub use dispatcher::{BackCommands, BackDispatcher, BackHandle};
pub use error::BackError;
pub use event::{BackEvent, BackPhase, GesturePosition, SwipeEdge};

#[cfg(test)]
mod tests;
