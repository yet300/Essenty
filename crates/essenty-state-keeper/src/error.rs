use thiserror::Error;

/// Typed errors returned by [`crate::StateKeeper`].
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
