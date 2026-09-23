use std::collections::BTreeMap;

use essenty_state_keeper::{StateKeeper, StateKeeperError};
use web_sys::{Storage, window};

use crate::storage::{decode_bytes, encode_bytes};
use crate::{StorageArea, StorageKey};

/// Failure to access or decode browser storage.
#[derive(Debug)]
pub enum BrowserStorageError {
    /// No browser window or requested storage area is available.
    Unavailable,
    /// The browser rejected a storage operation, for example due to quota.
    Access,
    /// An entry in this namespace does not contain valid Essenty hex bytes.
    InvalidEncoding(String),
    /// A Rust state provider failed while saving.
    State(StateKeeperError),
}

impl std::fmt::Display for BrowserStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("browser storage unavailable"),
            Self::Access => f.write_str("browser storage operation failed"),
            Self::InvalidEncoding(key) => write!(f, "invalid stored bytes for key '{key}'"),
            Self::State(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for BrowserStorageError {}

impl From<StateKeeperError> for BrowserStorageError {
    fn from(error: StateKeeperError) -> Self {
        Self::State(error)
    }
}

/// Explicit state persistence in `sessionStorage` or `localStorage`.
///
/// Each namespaced key holds lowercase hex bytes. Save replaces the contents
/// of this namespace, including removal of keys absent from the new snapshot.
/// Web Storage has no transaction support, so a failed write may leave a
/// partial snapshot and the error is returned to the caller.
#[derive(Debug)]
pub struct BrowserStorage {
    key: StorageKey,
    storage: Storage,
}

impl BrowserStorage {
    /// Opens the selected browser storage area.
    ///
    /// # Errors
    /// Returns an error when the window or storage area is unavailable.
    pub fn new(key: StorageKey) -> Result<Self, BrowserStorageError> {
        let window = window().ok_or(BrowserStorageError::Unavailable)?;
        let storage = match key.clone().area() {
            StorageArea::Session => window.session_storage(),
            StorageArea::Local => window.local_storage(),
        }
        .map_err(|_| BrowserStorageError::Access)?
        .ok_or(BrowserStorageError::Unavailable)?;
        Ok(Self { key, storage })
    }

    /// Loads all state entries in this namespace.
    ///
    /// # Errors
    /// Returns an error on storage access or malformed stored bytes.
    pub fn load(&self) -> Result<BTreeMap<String, Vec<u8>>, BrowserStorageError> {
        let prefix = self.key.prefix();
        let mut restored = BTreeMap::new();
        for index in 0..self.storage.length().map_err(|_| BrowserStorageError::Access)? {
            let Some(qualified) =
                self.storage.key(index).map_err(|_| BrowserStorageError::Access)?
            else {
                continue;
            };
            let Some(key) = qualified.strip_prefix(&prefix) else {
                continue;
            };
            if let Some(value) =
                self.storage.get_item(&qualified).map_err(|_| BrowserStorageError::Access)?
            {
                let bytes = decode_bytes(&value)
                    .ok_or_else(|| BrowserStorageError::InvalidEncoding(key.to_owned()))?;
                restored.insert(key.to_owned(), bytes);
            }
        }
        Ok(restored)
    }

    /// Creates a core keeper preloaded with the persisted bytes.
    ///
    /// # Errors
    /// Returns an error on storage access or malformed stored bytes.
    pub fn load_keeper(&self) -> Result<StateKeeper, BrowserStorageError> {
        Ok(StateKeeper::with_restored(self.load()?))
    }

    /// Replaces this namespace with a saved state map.
    ///
    /// # Errors
    /// Returns an error if a browser storage operation fails.
    pub fn save(&self, saved: &BTreeMap<String, Vec<u8>>) -> Result<(), BrowserStorageError> {
        let prefix = self.key.prefix();
        let mut stale = Vec::new();
        for index in 0..self.storage.length().map_err(|_| BrowserStorageError::Access)? {
            let Some(qualified) =
                self.storage.key(index).map_err(|_| BrowserStorageError::Access)?
            else {
                continue;
            };
            if let Some(key) = qualified.strip_prefix(&prefix) {
                if !saved.contains_key(key) {
                    stale.push(qualified);
                }
            }
        }
        for qualified in stale {
            self.storage.remove_item(&qualified).map_err(|_| BrowserStorageError::Access)?;
        }
        for (key, bytes) in saved {
            self.storage
                .set_item(&self.key.qualified(key), &encode_bytes(bytes))
                .map_err(|_| BrowserStorageError::Access)?;
        }
        Ok(())
    }

    /// Saves the registered providers and remaining restored state.
    ///
    /// # Errors
    /// Returns an error if a provider or browser storage operation fails.
    pub fn save_keeper(&self, keeper: &StateKeeper) -> Result<(), BrowserStorageError> {
        self.save(&keeper.save()?)
    }
}
