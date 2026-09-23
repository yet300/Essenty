use crate::LifecycleState;
use thiserror::Error;

/// Errors returned for invalid lifecycle transitions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LifecycleError {
    /// The requested transition is not allowed from the current state.
    #[error("invalid lifecycle transition from {from:?} to {to:?}")]
    InvalidTransition {
        /// State the registry was in.
        from: LifecycleState,
        /// State that was requested.
        to: LifecycleState,
    },
    /// The registry is already destroyed; it accepts no further transitions.
    #[error("lifecycle is already destroyed")]
    AlreadyDestroyed,
}
