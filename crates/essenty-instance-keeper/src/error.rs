use thiserror::Error;

/// Typed errors returned by [`crate::InstanceKeeper`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstanceKeeperError {
    /// The key is retained but holds a different type than requested.
    #[error("type mismatch for instance key '{0}'")]
    TypeMismatch(String),
    /// The keeper's scope has ended; new values cannot be retained.
    #[error("instance keeper is destroyed")]
    Destroyed,
}
