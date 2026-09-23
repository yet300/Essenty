//! Umbrella facade for the Essenty-inspired Rust runtime.
//!
//! Depending on [`essenty`](self) alone gives access to the four pure Rust
//! core crates. No implementation lives here; each module re-exports its
//! backing crate.
//!
//! ```toml
//! essenty = "0.1"
//! ```
//!
//! ```rust
//! use essenty::{LifecycleRegistry, StateKeeper, InstanceKeeper, BackDispatcher};
//!
//! let lifecycle = LifecycleRegistry::new();
//! lifecycle.create().unwrap();
//!
//! let mut keeper = StateKeeper::new();
//! keeper.register("k", || vec![1]).unwrap();
//!
//! let mut instances = InstanceKeeper::new();
//! let value = instances.get_or_create("v", || 42_u32).unwrap();
//! assert_eq!(*value, 42);
//!
//! let mut back = BackDispatcher::new();
//! assert!(!back.back());
//! ```

/// Application/component lifecycle.
pub mod lifecycle {
    pub use essenty_lifecycle::*;
}

/// State preservation with pluggable `serde` codecs.
pub mod state_keeper {
    pub use essenty_state_keeper::*;
}

/// Retained objects with [`std::rc::Rc`] ownership and [`Drop`] cleanup.
pub mod instance_keeper {
    pub use essenty_instance_keeper::*;
}

/// Back-event dispatch with predictive gesture support.
pub mod back_handler {
    pub use essenty_back_handler::*;
}

pub use essenty_back_handler::{BackDispatcher, BackError, BackEvent, BackHandle, BackPhase};
pub use essenty_instance_keeper::{InstanceKeeper, InstanceKeeperError};
pub use essenty_lifecycle::{LifecycleError, LifecycleRegistry, LifecycleState, Subscription};
pub use essenty_state_keeper::{StateKeeper, StateKeeperError};
