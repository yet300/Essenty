use essenty_lifecycle::LifecycleState;
use thiserror::Error;

/// Error returned when [`crate::LifecycleScope::spawn`] or
/// [`crate::LifecycleScope::spawn_local`] is called after the lifecycle has
/// been destroyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SpawnError {
    /// The lifecycle is already destroyed; the task was not started.
    #[error("lifecycle is already destroyed")]
    Destroyed,
}

/// Error returned by [`crate::LifecycleScope::try_new`] when no Tokio runtime
/// is active on the calling thread.
#[derive(Debug, Error)]
pub enum ScopeError {
    /// No Tokio runtime is active, so the scope cannot pick an ambient handle.
    /// Construct the scope with [`crate::LifecycleScope::with_handle`] and an
    /// explicit handle instead.
    #[error("no Tokio runtime is active on this thread")]
    NoRuntime(#[from] tokio::runtime::TryCurrentError),
}

/// Error returned by [`crate::repeat_on_lifecycle`] and
/// [`crate::repeat_on_lifecycle_local`] for an unusable minimum active state.
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
