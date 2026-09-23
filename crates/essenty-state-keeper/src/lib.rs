//! Pure Rust state preservation abstraction.
//!
//! The keeper is deliberately format-agnostic: providers produce opaque
//! `Vec<u8>` payloads and restored state is an ordered map of key to bytes.
//! How those bytes are produced is the caller's choice (`serde_json`,
//! `postcard`, `bincode`, platform parcel code, ...), so the storage format
//! can evolve without changing this crate.
//!
//! [`serde`] enters through the generic helper methods
//! ([`StateKeeper::register_value`], [`StateKeeper::consume_value`]), which
//! are bounded by [`serde::Serialize`] / [`serde::Deserialize`] but take an
//! explicit caller-supplied `encode` / `decode` function. No JSON (or any
//! other format) is hard-coded in library code; tests use `serde_json` only
//! as one example codec via a dev-dependency.
//!
//! Responsibilities:
//!
//! - register state providers under unique keys
//! - consume restored state exactly once per key
//! - save all registered providers into a deterministic ordered map
//! - reject duplicate keys with a typed error
//! - support explicit unregistration
//!
//! The keeper is single-threaded (`!Send`, `!Sync`) and performs no I/O. It
//! currently requires `std`, but only `alloc`/`core` containers are used, so
//! future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_state_keeper::StateKeeper;
//!
//! let mut keeper = StateKeeper::new();
//! keeper.register("counter", || 41_u32.to_le_bytes().to_vec()).unwrap();
//! let saved = keeper.save().unwrap();
//!
//! let mut restored = StateKeeper::with_restored(saved);
//! let bytes = restored.consume_bytes("counter").unwrap();
//! assert_eq!(u32::from_le_bytes(bytes.try_into().unwrap()), 41);
//! ```

mod error;
mod keeper;

pub use error::StateKeeperError;
pub use keeper::StateKeeper;

#[cfg(test)]
mod tests;
