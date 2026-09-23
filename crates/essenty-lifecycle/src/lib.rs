//! Pure Rust lifecycle abstraction.
//!
//! Models an application or component lifecycle as an ordered set of states
//! with validated transitions and observer subscriptions.
//!
//! Job of this crate:
//!
//! - track the current [`LifecycleState`]
//! - validate deterministic state transitions
//! - notify observers in subscription order while advancing and reverse order while retreating
//! - unsubscribe automatically via RAII [`Subscription`] guards
//! - expose a manually controlled [`LifecycleRegistry`] useful for tests
//!   and for platform adapters (Android, Apple, Web) that forward native
//!   lifecycle events into the pure Rust core.
//!
//! The registry is single-threaded (`!Send`, `!Sync`) by design. It uses
//! `Rc`/`RefCell` rather than `Arc`/`Mutex` so it works naturally on
//! WebAssembly and in single-threaded UI runtimes. Multithreaded hosts can
//! wrap the registry in a `Mutex` or confine it to one thread.
//!
//! Currently requires `std`; only `alloc`/`core` features are used for the
//! retained data, so future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
//!
//! let lifecycle = LifecycleRegistry::new();
//! let _guard = lifecycle.subscribe(|state| {
//!     println!("lifecycle moved to {state:?}");
//! });
//!
//! lifecycle.create().unwrap();
//! lifecycle.start().unwrap();
//! lifecycle.resume().unwrap();
//! assert_eq!(lifecycle.state(), LifecycleState::Resumed);
//! ```

mod error;
mod registry;
mod state;

pub use error::LifecycleError;
pub use registry::{LifecycleRegistry, Subscription};
pub use state::LifecycleState;

#[cfg(test)]
mod tests;
