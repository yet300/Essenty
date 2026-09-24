use essenty_lifecycle::LifecycleState;
use thiserror::Error;

/// Error returned when [`crate::LifecycleScope::spawn`] is called after the
/// lifecycle has been destroyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SpawnError {
    /// The lifecycle is already destroyed; the task was not started.
    #[error("lifecycle is already destroyed")]
    Destroyed,
}

/// Error returned by [`crate::repeat_on_lifecycle`] for an unusable minimum
/// active state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RepeatError {
    /// `INITIALIZED` can never be a minimum active state (matches upstream
    /// `require(minActiveState != INITIALIZED)`).
    #[error("INITIALIZED cannot be the minimum active state")]
    InvalidMinState {
        /// The rejected minimum state (always `INITIALIZED`).
        min_state: LifecycleState,
    },
}
