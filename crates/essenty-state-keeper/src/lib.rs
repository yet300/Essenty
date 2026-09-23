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

use std::collections::BTreeMap;

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

/// Typed errors returned by [`StateKeeper`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StateKeeperError {
    /// A provider is already registered under this key.
    #[error("duplicate state key: '{0}'")]
    DuplicateKey(String),
    /// Encoding a [`serde::Serialize`] value failed.
    #[error("encode failed for key '{key}': {reason}")]
    Encode {
        /// Key being saved.
        key: String,
        /// Codec-provided reason (format-specific).
        reason: String,
    },
    /// Decoding restored bytes failed.
    #[error("decode failed for key '{key}': {reason}")]
    Decode {
        /// Key being consumed.
        key: String,
        /// Codec-provided reason (format-specific).
        reason: String,
    },
}

/// A registered state provider evaluated at save time.
type Provider = Box<dyn Fn() -> Result<Vec<u8>, StateKeeperError>>;

/// Pure Rust state keeper.
///
/// `restored` holds bytes supplied by the host (process recreation,
/// navigation restore, ...). Each key can be consumed at most once, mirroring
/// Essenty semantics where restored state is single-shot. Providers supply
/// fresh bytes at [`StateKeeper::save`] time.
#[derive(Default)]
pub struct StateKeeper {
    restored: BTreeMap<String, Vec<u8>>,
    providers: BTreeMap<String, Provider>,
}

impl std::fmt::Debug for StateKeeper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Providers intentionally omitted: `dyn Fn` has no `Debug`.
        f.debug_struct("StateKeeper")
            .field("pending_count", &self.restored.len())
            .field("provider_count", &self.providers.len())
            .finish_non_exhaustive()
    }
}

impl StateKeeper {
    /// Creates an empty keeper with no restored state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a keeper preloaded with restored state.
    ///
    /// Keys are consumed via [`StateKeeper::consume_bytes`] or
    /// [`StateKeeper::consume_value`].
    #[must_use]
    pub fn with_restored(restored: BTreeMap<String, Vec<u8>>) -> Self {
        Self { restored, providers: BTreeMap::new() }
    }

    /// Returns `true` if a provider is registered under `key`.
    #[must_use]
    pub fn has_provider(&self, key: &str) -> bool {
        self.providers.contains_key(key)
    }

    /// Returns `true` if unconsumed restored state remains under `key`.
    #[must_use]
    pub fn has_consumable(&self, key: &str) -> bool {
        self.restored.contains_key(key)
    }

    /// Number of registered providers.
    #[must_use]
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Number of unconsumed restored entries.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.restored.len()
    }

    /// Registers a byte-producing provider.
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::DuplicateKey`] if `key` is taken.
    pub fn register(
        &mut self,
        key: &str,
        provider: impl Fn() -> Vec<u8> + 'static,
    ) -> Result<(), StateKeeperError> {
        if self.providers.contains_key(key) {
            return Err(StateKeeperError::DuplicateKey(key.to_owned()));
        }
        self.providers.insert(key.to_owned(), Box::new(move || Ok(provider())));
        Ok(())
    }

    /// Registers a provider for a [`serde::Serialize`] value.
    ///
    /// `supplier` produces the live value at save time; `encode` serializes
    /// it with whatever format the caller chooses (JSON, Postcard, ...).
    /// Encoding failures surface at [`StateKeeper::save`] time as
    /// [`StateKeeperError::Encode`].
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::DuplicateKey`] if `key` is taken.
    pub fn register_value<T>(
        &mut self,
        key: &str,
        supplier: impl Fn() -> T + 'static,
        encode: impl Fn(&T) -> Result<Vec<u8>, String> + 'static,
    ) -> Result<(), StateKeeperError>
    where
        T: Serialize + 'static,
    {
        if self.providers.contains_key(key) {
            return Err(StateKeeperError::DuplicateKey(key.to_owned()));
        }
        let owned = key.to_owned();
        self.providers.insert(
            owned.clone(),
            Box::new(move || {
                encode(&supplier())
                    .map_err(|reason| StateKeeperError::Encode { key: owned.clone(), reason })
            }),
        );
        Ok(())
    }

    /// Removes the provider registered under `key`. Returns `true` if one
    /// existed. Restored (unconsumed) state under the same key is untouched.
    pub fn unregister(&mut self, key: &str) -> bool {
        self.providers.remove(key).is_some()
    }

    /// Consumes restored bytes for `key` exactly once. Returns `None` when
    /// no restored state remains (absent or already consumed).
    pub fn consume_bytes(&mut self, key: &str) -> Option<Vec<u8>> {
        self.restored.remove(key)
    }

    /// Consumes and decodes restored state for `key` exactly once.
    ///
    /// Returns `Ok(None)` when nothing remains. Decoding is delegated to the
    /// caller-supplied `decode` function so any `serde` format works.
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::Decode`] when bytes exist but decoding
    /// fails.
    pub fn consume_value<T>(
        &mut self,
        key: &str,
        decode: impl FnOnce(&[u8]) -> Result<T, String>,
    ) -> Result<Option<T>, StateKeeperError>
    where
        T: DeserializeOwned,
    {
        let Some(bytes) = self.restored.remove(key) else {
            return Ok(None);
        };
        decode(&bytes)
            .map(Some)
            .map_err(|reason| StateKeeperError::Decode { key: key.to_owned(), reason })
    }

    /// Saves unconsumed restored entries and registered providers into a
    /// deterministic (key-ordered) map. Providers replace restored values
    /// with the same key. The first encoding failure
    /// aborts the save with [`StateKeeperError::Encode`].
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::Encode`] if any provider fails.
    pub fn save(&self) -> Result<BTreeMap<String, Vec<u8>>, StateKeeperError> {
        let mut out = self.restored.clone();
        for (key, provider) in &self.providers {
            out.insert(key.clone(), provider()?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Counter {
        value: u32,
    }

    fn json_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
        serde_json::to_vec(value).map_err(|e| e.to_string())
    }

    fn json_decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    #[test]
    fn empty_save_yields_empty_map() {
        let keeper = StateKeeper::new();
        assert!(keeper.save().unwrap().is_empty());
        assert_eq!(keeper.provider_count(), 0);
        assert_eq!(keeper.pending_count(), 0);
    }

    #[test]
    fn bytes_round_trip() {
        let mut keeper = StateKeeper::new();
        keeper.register("counter", || 41_u32.to_le_bytes().to_vec()).unwrap();
        let saved = keeper.save().unwrap();

        let mut restored = StateKeeper::with_restored(saved);
        let bytes = restored.consume_bytes("counter").unwrap();
        assert_eq!(u32::from_le_bytes(bytes.try_into().unwrap()), 41);
    }

    #[test]
    fn serde_value_round_trip_with_pluggable_codec() {
        let mut keeper = StateKeeper::new();
        keeper.register_value("counter", || Counter { value: 7 }, json_encode).unwrap();
        let saved = keeper.save().unwrap();

        let mut restored = StateKeeper::with_restored(saved);
        let value: Option<Counter> = restored.consume_value("counter", json_decode).unwrap();
        assert_eq!(value, Some(Counter { value: 7 }));
    }

    #[test]
    fn multiple_keys_save_in_order() {
        let mut keeper = StateKeeper::new();
        keeper.register("b", || vec![2]).unwrap();
        keeper.register("a", || vec![1]).unwrap();
        keeper.register("c", || vec![3]).unwrap();
        let saved = keeper.save().unwrap();
        let keys: Vec<&str> = saved.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn consume_is_single_shot() {
        let mut restored =
            StateKeeper::with_restored(BTreeMap::from([("k".to_owned(), vec![1, 2, 3])]));
        assert!(restored.has_consumable("k"));
        assert_eq!(restored.consume_bytes("k"), Some(vec![1, 2, 3]));
        assert!(!restored.has_consumable("k"));
        assert_eq!(restored.consume_bytes("k"), None);

        let mut restored = StateKeeper::with_restored(BTreeMap::from([(
            "v".to_owned(),
            serde_json::to_vec(&Counter { value: 1 }).unwrap(),
        )]));
        let first: Option<Counter> = restored.consume_value("v", json_decode).unwrap();
        assert_eq!(first, Some(Counter { value: 1 }));
        let second: Option<Counter> = restored.consume_value("v", json_decode).unwrap();
        assert_eq!(second, None);
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut keeper = StateKeeper::new();
        keeper.register("k", || vec![1]).unwrap();
        assert_eq!(
            keeper.register("k", || vec![2]).unwrap_err(),
            StateKeeperError::DuplicateKey("k".to_owned())
        );
        assert_eq!(
            keeper.register_value("k", || 1_u8, json_encode).unwrap_err(),
            StateKeeperError::DuplicateKey("k".to_owned())
        );
        // Original provider is intact.
        assert_eq!(keeper.save().unwrap()["k"], vec![1]);
    }

    #[test]
    fn unregister_removes_provider_but_keeps_restored() {
        let mut keeper = StateKeeper::with_restored(BTreeMap::from([("k".to_owned(), vec![9])]));
        keeper.register("k", || vec![1]).unwrap();
        assert!(keeper.has_provider("k"));
        assert!(keeper.unregister("k"));
        assert!(!keeper.has_provider("k"));
        assert!(!keeper.unregister("k"));
        assert_eq!(keeper.save().unwrap().get("k"), Some(&vec![9]));
        // Restored bytes survive unregistration.
        assert_eq!(keeper.consume_bytes("k"), Some(vec![9]));
    }

    #[test]
    fn decode_failure_returns_typed_error() {
        let mut restored =
            StateKeeper::with_restored(BTreeMap::from([("v".to_owned(), vec![0xFF])]));
        let err = restored.consume_value::<Counter>("v", json_decode).unwrap_err();
        assert!(matches!(err, StateKeeperError::Decode { .. }));
    }

    #[test]
    fn encode_failure_surfaces_at_save() {
        let mut keeper = StateKeeper::new();
        keeper.register_value("bad", || Counter { value: 1 }, |_| Err("boom".to_owned())).unwrap();
        let err = keeper.save().unwrap_err();
        assert_eq!(
            err,
            StateKeeperError::Encode { key: "bad".to_owned(), reason: "boom".to_owned() }
        );
    }

    #[test]
    fn unconsumed_restored_values_survive_another_save() {
        let mut keeper = StateKeeper::with_restored(BTreeMap::from([
            ("old".to_owned(), vec![1]),
            ("replaced".to_owned(), vec![2]),
            ("consumed".to_owned(), vec![3]),
        ]));
        assert_eq!(keeper.consume_bytes("consumed"), Some(vec![3]));
        keeper.register("replaced", || vec![9]).unwrap();
        let saved = keeper.save().unwrap();
        assert_eq!(saved.get("old"), Some(&vec![1]));
        assert_eq!(saved.get("replaced"), Some(&vec![9]));
        assert!(!saved.contains_key("consumed"));
    }
}
