//! Android adapter for the Essenty Rust runtime.
//!
//! Bootstrap scope: this crate contains **host-compilable, pure Rust**
//! adapters that map Android concepts onto the core crates. There is no JNI
//! layer yet — the structs here are designed so a future `jni` integration
//! can drive them from `Activity` callbacks without changing callers.
//!
//! Planned (not yet implemented):
//!
//! - `Activity` lifecycle callbacks → [`AndroidLifecycle`]
//! - `SavedStateRegistry` save/restore → [`AndroidStateHost`]
//! - `ViewModelStore` retention → core `InstanceKeeper` (already usable)
//! - `OnBackPressedDispatcher` + Android Predictive Back → [`AndroidBackBridge`]
//!
//! All Android/JNI dependencies must live behind
//! `target.'cfg(target_os = "android")'.dependencies` in this crate and must
//! never leak into the core crates.
//!
//! On Android, the optional `native-activity` feature provides
//! `NativeActivityLifecycle`, which drives the registry while polling an
//! `android-activity` event loop. No application-written Java is needed for
//! that host model.
//!
//! # Example
//!
//! ```rust
//! use essenty_android::AndroidLifecycle;
//!
//! let host = AndroidLifecycle::new();
//! host.on_create();
//! host.on_start();
//! host.on_resume();
//! assert!(host.registry().state().is_resumed());
//! ```

use std::collections::BTreeMap;

use essenty_back_handler::BackDispatcher;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use essenty_state_keeper::StateKeeper;

#[cfg(all(target_os = "android", feature = "native-activity"))]
mod native_activity;

#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use native_activity::NativeActivityLifecycle;

/// Forwards Android `Activity` lifecycle callbacks into a [`LifecycleRegistry`].
///
/// Each method is idempotent with respect to its target state: calling
/// `on_resume` when already resumed is a no-op returning `Ok(())`, which
/// keeps duplicate platform callbacks harmless. Out-of-order transitions
/// are walked through intermediate states by the core registry.
#[derive(Debug, Clone, Default)]
pub struct AndroidLifecycle {
    registry: LifecycleRegistry,
}

impl AndroidLifecycle {
    /// Creates a host in `Initialized`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the underlying registry (for observers and state checks).
    #[must_use]
    pub fn registry(&self) -> &LifecycleRegistry {
        &self.registry
    }

    /// Maps to `Activity.onCreate`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_create(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        if self.registry.state() == LifecycleState::Initialized {
            self.registry.create()
        } else {
            self.registry.move_to(LifecycleState::Created)
        }
    }

    /// Maps to `Activity.onStart`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_start(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Started)
    }

    /// Maps to `Activity.onResume`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_resume(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Resumed)
    }

    /// Maps to `Activity.onPause`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_pause(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        // Pause from Resumed goes to Started; from Started it is a no-op.
        self.registry.move_to(LifecycleState::Started)
    }

    /// Maps to `Activity.onStop`.
    ///
    /// # Errors
    ///
    /// Propagates registry transition errors (e.g. already destroyed).
    pub fn on_stop(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.move_to(LifecycleState::Created)
    }

    /// Maps to `Activity.onDestroy`.
    ///
    /// # Errors
    ///
    /// Returns [`AlreadyDestroyed`](essenty_lifecycle::LifecycleError::AlreadyDestroyed)
    /// when the registry is already destroyed.
    pub fn on_destroy(&self) -> Result<(), essenty_lifecycle::LifecycleError> {
        self.registry.destroy()
    }
}

/// Hosts saved-state bytes across Android process recreation.
///
/// Thin wrapper around [`StateKeeper`] documenting the intended
/// `SavedStateRegistry` wiring: `consume` restored bytes once after
/// recreation, `register` live providers, then `perform_save` when the
/// platform requests a snapshot. Byte format stays caller-defined.
#[derive(Debug, Default)]
pub struct AndroidStateHost {
    keeper: StateKeeper,
}

impl AndroidStateHost {
    /// Empty host with no restored state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Host preloaded with bytes from `SavedStateRegistry.consumeRestoredStateForKey`
    /// equivalents.
    #[must_use]
    pub fn with_restored(restored: BTreeMap<String, Vec<u8>>) -> Self {
        Self { keeper: StateKeeper::with_restored(restored) }
    }

    /// Borrows the underlying keeper mutably for provider registration and
    /// single-shot consumption.
    #[must_use]
    pub fn keeper_mut(&mut self) -> &mut StateKeeper {
        &mut self.keeper
    }

    /// Borrows the underlying keeper.
    #[must_use]
    pub fn keeper(&self) -> &StateKeeper {
        &self.keeper
    }

    /// Produces the snapshot to hand to the platform
    /// (`onSaveInstanceState` equivalent).
    ///
    /// # Errors
    ///
    /// Propagates [`StateKeeperError::Encode`](essenty_state_keeper::StateKeeperError::Encode)
    /// from failing providers.
    pub fn perform_save(
        &self,
    ) -> Result<BTreeMap<String, Vec<u8>>, essenty_state_keeper::StateKeeperError> {
        self.keeper.save()
    }
}

/// Forwards `OnBackPressedDispatcher` / Predictive Back events into a core
/// [`BackDispatcher`].
///
/// The future JNI layer will call `handle_back_pressed`,
/// `handle_gesture_start`, and so on from dispatcher callbacks; until then
/// this bridge documents the mapping and stays fully testable on the host.
#[derive(Debug, Default)]
pub struct AndroidBackBridge {
    dispatcher: BackDispatcher,
}

impl AndroidBackBridge {
    /// Creates an empty bridge.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the dispatcher mutably for callback registration.
    #[must_use]
    pub fn dispatcher_mut(&mut self) -> &mut BackDispatcher {
        &mut self.dispatcher
    }

    /// Borrows the dispatcher.
    #[must_use]
    pub fn dispatcher(&self) -> &BackDispatcher {
        &self.dispatcher
    }

    /// `OnBackPressedDispatcher.onBackPressed` equivalent.
    pub fn handle_back_pressed(&mut self) -> bool {
        self.dispatcher.back()
    }

    /// Predictive Back `onBackStarted` equivalent.
    pub fn handle_gesture_start(&mut self) -> bool {
        self.dispatcher.predictive_start()
    }

    /// Predictive Back `onBackProgressed` equivalent.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`](essenty_back_handler::BackError)
    /// when no gesture is in flight.
    pub fn handle_gesture_progress(
        &mut self,
        progress: f32,
    ) -> Result<bool, essenty_back_handler::BackError> {
        self.dispatcher.predictive_progress(progress)
    }

    /// Predictive Back `onBackCancelled` equivalent.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`](essenty_back_handler::BackError)
    /// when no gesture is in flight.
    pub fn handle_gesture_cancel(&mut self) -> Result<bool, essenty_back_handler::BackError> {
        self.dispatcher.predictive_cancel()
    }

    /// Predictive Back `onBackInvoked` equivalent (gesture completion or
    /// plain system-back invocation while a gesture is claimed).
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`](essenty_back_handler::BackError)
    /// when no gesture is in flight.
    pub fn handle_gesture_invoke(&mut self) -> Result<bool, essenty_back_handler::BackError> {
        self.dispatcher.predictive_invoke()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_lifecycle_maps_to_registry() {
        let host = AndroidLifecycle::new();
        host.on_create().unwrap();
        host.on_start().unwrap();
        host.on_resume().unwrap();
        assert!(host.registry().state().is_resumed());
        host.on_pause().unwrap();
        assert_eq!(host.registry().state(), LifecycleState::Started);
        host.on_stop().unwrap();
        assert_eq!(host.registry().state(), LifecycleState::Created);
        host.on_destroy().unwrap();
        assert!(host.registry().state().is_destroyed());
    }

    #[test]
    fn duplicate_callbacks_are_harmless() {
        let host = AndroidLifecycle::new();
        host.on_create().unwrap();
        host.on_create().unwrap();
        host.on_start().unwrap();
        host.on_start().unwrap();
        assert_eq!(host.registry().state(), LifecycleState::Started);
    }

    #[test]
    fn state_host_save_round_trip() {
        let mut host = AndroidStateHost::new();
        host.keeper_mut().register("k", || vec![1, 2]).unwrap();
        let saved = host.perform_save().unwrap();
        let mut restored = AndroidStateHost::with_restored(saved);
        assert_eq!(restored.keeper_mut().consume_bytes("k"), Some(vec![1, 2]));
    }

    #[test]
    fn back_bridge_forwards_press_and_gesture() {
        let mut bridge = AndroidBackBridge::new();
        assert!(!bridge.handle_back_pressed());
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0_u32));
        let probe = std::rc::Rc::clone(&calls);
        bridge.dispatcher_mut().register(0, true, move |_| *probe.borrow_mut() += 1);
        assert!(bridge.handle_back_pressed());
        assert!(bridge.handle_gesture_start());
        assert!(bridge.handle_gesture_progress(0.5).unwrap());
        assert!(bridge.handle_gesture_invoke().unwrap());
        assert_eq!(*calls.borrow(), 4);
    }
}
