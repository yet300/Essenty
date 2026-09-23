use crate::{
    BackDispatcher, BackError, InstanceKeeper, LifecycleError, LifecycleRegistry, LifecycleState,
    StateKeeper, StateKeeperError,
};
use std::collections::BTreeMap;

/// Platform-neutral input to an application runtime. Platform adapters
/// translate native callbacks into one of these coarse events.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformEvent {
    /// Drive the root lifecycle; child propagation remains in Rust.
    Lifecycle(LifecycleState),
    /// Ordinary system back action.
    BackPressed,
    /// Predictive back gesture began.
    PredictiveStart,
    /// Optional core progress; platform animations may remain native.
    PredictiveProgress(f32),
    /// Predictive gesture was cancelled.
    PredictiveCancel,
    /// Predictive gesture committed.
    PredictiveCommit,
    /// Capture one opaque, keyed state snapshot for the host.
    SaveState,
}

/// One response to one platform event. The adapter can inspect the result
/// without calling back into Rust for individual component properties.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DispatchResult {
    /// Whether a back action or gesture phase found a handler.
    pub back_handled: Option<bool>,
    /// Saved state, present only for [`PlatformEvent::SaveState`].
    pub saved_state: Option<BTreeMap<String, Vec<u8>>>,
    /// Current aggregate back availability after dispatch.
    pub can_handle_back: bool,
}

/// Failure from one runtime dispatch.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// Invalid lifecycle transition.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    /// Invalid predictive back sequence.
    #[error(transparent)]
    Back(#[from] BackError),
    /// State provider failed while saving.
    #[error(transparent)]
    State(#[from] StateKeeperError),
}

/// Owns the Essenty primitives behind one platform event entry point.
///
/// Rust components receive references to the subsystems during graph setup;
/// platform bindings retain one runtime handle and dispatch coarse events.
#[derive(Debug, Default)]
pub struct Runtime {
    lifecycle: LifecycleRegistry,
    state_keeper: StateKeeper,
    instance_keeper: InstanceKeeper,
    back_dispatcher: BackDispatcher,
}

impl Runtime {
    /// New runtime with no restored state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// New runtime with the host's opaque restored state map.
    #[must_use]
    pub fn with_restored(restored: BTreeMap<String, Vec<u8>>) -> Self {
        Self { state_keeper: StateKeeper::with_restored(restored), ..Self::new() }
    }

    /// Root lifecycle handle for Rust components.
    #[must_use]
    pub fn lifecycle(&self) -> &LifecycleRegistry {
        &self.lifecycle
    }

    /// State keeper for Rust component registration and consumption.
    #[must_use]
    pub fn state_keeper_mut(&mut self) -> &mut StateKeeper {
        &mut self.state_keeper
    }

    /// Retained instance keeper for Rust components.
    #[must_use]
    pub fn instance_keeper_mut(&mut self) -> &mut InstanceKeeper {
        &mut self.instance_keeper
    }

    /// Back dispatcher for Rust component registration.
    #[must_use]
    pub fn back_dispatcher_mut(&mut self) -> &mut BackDispatcher {
        &mut self.back_dispatcher
    }

    /// Process a platform event synchronously and return its complete result.
    ///
    /// # Errors
    /// Returns a typed error for invalid transitions, gesture sequences, or
    /// state provider failures.
    pub fn dispatch(&mut self, event: PlatformEvent) -> Result<DispatchResult, RuntimeError> {
        let mut result = DispatchResult::default();
        match event {
            PlatformEvent::Lifecycle(state) => {
                self.lifecycle.move_to(state)?;
                if state == LifecycleState::Destroyed {
                    self.instance_keeper.destroy_all();
                }
            }
            PlatformEvent::BackPressed => result.back_handled = Some(self.back_dispatcher.back()),
            PlatformEvent::PredictiveStart => {
                result.back_handled = Some(self.back_dispatcher.predictive_start());
            }
            PlatformEvent::PredictiveProgress(progress) => {
                result.back_handled = Some(self.back_dispatcher.predictive_progress(progress)?);
            }
            PlatformEvent::PredictiveCancel => {
                result.back_handled = Some(self.back_dispatcher.predictive_cancel()?);
            }
            PlatformEvent::PredictiveCommit => {
                result.back_handled = Some(self.back_dispatcher.predictive_invoke()?);
            }
            PlatformEvent::SaveState => result.saved_state = Some(self.state_keeper.save()?),
        }
        result.can_handle_back = self.back_dispatcher.can_handle();
        Ok(result)
    }
}
