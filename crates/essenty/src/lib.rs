//! Rust-native Essenty primitives. Each subsystem can be used independently.
//!
//! Depending on [`essenty`](self) alone gives access to the four pure Rust
//! core crates. An optional event-driven host is available with the `runtime`
//! feature.
//!
//! ```toml
//! essenty = "0.1"
//! ```
//!
//! ```rust
//! use essenty::{LifecycleRegistry, StateKeeper, InstanceKeeper, BackDispatcher};
//!
//! let lifecycle = LifecycleRegistry::new();
//! lifecycle.create().unwrap();
//!
//! let mut keeper = StateKeeper::new();
//! keeper.register("k", || vec![1]).unwrap();
//!
//! let mut instances = InstanceKeeper::new();
//! let value = instances.get_or_create("v", || 42_u32).unwrap();
//! assert_eq!(*value, 42);
//!
//! let mut back = BackDispatcher::new();
//! assert!(!back.back());
//! ```

/// Application/component lifecycle.
pub mod lifecycle {
    pub use essenty_lifecycle::*;
}

/// State preservation with pluggable `serde` codecs.
pub mod state_keeper {
    pub use essenty_state_keeper::*;
}

/// Retained objects with [`std::rc::Rc`] ownership and [`Drop`] cleanup.
pub mod instance_keeper {
    pub use essenty_instance_keeper::*;
}

/// Back-event dispatch with predictive gesture support.
pub mod back_handler {
    pub use essenty_back_handler::*;
}

pub use essenty_back_handler::{BackDispatcher, BackError, BackEvent, BackHandle, BackPhase};
pub use essenty_instance_keeper::{InstanceKeeper, InstanceKeeperError};
pub use essenty_lifecycle::{LifecycleError, LifecycleRegistry, LifecycleState, Subscription};
pub use essenty_state_keeper::{StateKeeper, StateKeeperError};

#[cfg(feature = "runtime")]
use std::collections::BTreeMap;

/// Platform-neutral input to an application runtime. Platform adapters
/// translate native callbacks into one of these coarse events.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(feature = "runtime")]
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
#[cfg(feature = "runtime")]
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
#[cfg(feature = "runtime")]
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
#[cfg(feature = "runtime")]
#[derive(Debug, Default)]
pub struct Runtime {
    lifecycle: LifecycleRegistry,
    state_keeper: StateKeeper,
    instance_keeper: InstanceKeeper,
    back_dispatcher: BackDispatcher,
}

#[cfg(feature = "runtime")]
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

#[cfg(all(test, feature = "runtime"))]
mod runtime_tests {
    use super::*;

    #[test]
    fn single_dispatch_routes_lifecycle_back_and_state() {
        let mut runtime = Runtime::new();
        runtime.state_keeper_mut().register("counter", || vec![7]).unwrap();
        runtime.back_dispatcher_mut().register(0, true, |_| {});
        runtime.dispatch(PlatformEvent::Lifecycle(LifecycleState::Resumed)).unwrap();
        assert_eq!(runtime.lifecycle().state(), LifecycleState::Resumed);
        let back = runtime.dispatch(PlatformEvent::BackPressed).unwrap();
        assert_eq!(back.back_handled, Some(true));
        assert!(back.can_handle_back);
        let saved = runtime.dispatch(PlatformEvent::SaveState).unwrap();
        assert_eq!(saved.saved_state.unwrap().get("counter"), Some(&vec![7]));
    }

    #[test]
    fn restored_state_is_consumed_inside_runtime() {
        let mut runtime = Runtime::with_restored(BTreeMap::from([("key".into(), vec![4])]));
        assert_eq!(runtime.state_keeper_mut().consume_bytes("key"), Some(vec![4]));
        assert!(
            runtime.dispatch(PlatformEvent::SaveState).unwrap().saved_state.unwrap().is_empty()
        );
    }

    #[test]
    fn terminal_lifecycle_event_ends_retained_scope() {
        let mut runtime = Runtime::new();
        runtime.instance_keeper_mut().get_or_create("model", || 1_u32).unwrap();
        runtime.dispatch(PlatformEvent::Lifecycle(LifecycleState::Destroyed)).unwrap();
        assert!(runtime.instance_keeper_mut().is_destroyed());
        assert_eq!(
            runtime.instance_keeper_mut().get_or_create("new", || 2_u32).unwrap_err(),
            InstanceKeeperError::Destroyed
        );
    }
}
