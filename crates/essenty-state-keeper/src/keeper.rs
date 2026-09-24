use crate::StateKeeperError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

type Provider = Box<dyn Fn() -> Result<Option<Vec<u8>>, StateKeeperError>>;

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

    /// Returns `true` if a provider is registered under `key`.
    ///
    /// Alias of [`StateKeeper::has_provider`] using the upstream
    /// `StateKeeper.isRegistered` name.
    #[must_use]
    pub fn is_registered(&self, key: &str) -> bool {
        self.has_provider(key)
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
        self.providers.insert(key.to_owned(), Box::new(move || Ok(Some(provider()))));
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
                    .map(Some)
                    .map_err(|reason| StateKeeperError::Encode { key: owned.clone(), reason })
            }),
        );
        Ok(())
    }

    /// Registers a byte-producing provider that may decline to supply state.
    ///
    /// This mirrors the upstream nullable supplier (`supplier: () -> T?`):
    /// returning `None` skips the key at [`StateKeeper::save`] time, leaving
    /// any unconsumed restored value under the same key intact.
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::DuplicateKey`] if `key` is taken.
    pub fn register_optional(
        &mut self,
        key: &str,
        provider: impl Fn() -> Option<Vec<u8>> + 'static,
    ) -> Result<(), StateKeeperError> {
        if self.providers.contains_key(key) {
            return Err(StateKeeperError::DuplicateKey(key.to_owned()));
        }
        self.providers.insert(key.to_owned(), Box::new(move || Ok(provider())));
        Ok(())
    }

    /// Registers a [`serde::Serialize`] provider that may decline to supply
    /// state, returning `None` from `supplier` to skip the key at save time.
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::DuplicateKey`] if `key` is taken.
    pub fn register_optional_value<T>(
        &mut self,
        key: &str,
        supplier: impl Fn() -> Option<T> + 'static,
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
                supplier()
                    .map(|value| {
                        encode(&value).map_err(|reason| StateKeeperError::Encode {
                            key: owned.clone(),
                            reason,
                        })
                    })
                    .transpose()
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
    /// deterministic (key-ordered) map. Providers returning bytes replace
    /// restored values with the same key; providers returning `None` (see
    /// [`StateKeeper::register_optional`]) skip the key, preserving any
    /// unconsumed restored value. The first encoding failure
    /// aborts the save with [`StateKeeperError::Encode`].
    ///
    /// # Errors
    ///
    /// Returns [`StateKeeperError::Encode`] if any provider fails.
    pub fn save(&self) -> Result<BTreeMap<String, Vec<u8>>, StateKeeperError> {
        let mut out = self.restored.clone();
        for (key, provider) in &self.providers {
            if let Some(bytes) = provider()? {
                out.insert(key.clone(), bytes);
            }
        }
        Ok(out)
    }
}
