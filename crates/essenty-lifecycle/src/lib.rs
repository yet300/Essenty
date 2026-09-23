//! Pure Rust lifecycle abstraction.
//!
//! Models an application or component lifecycle as an ordered set of states
//! with validated transitions and observer subscriptions.
//!
//! Job of this crate:
//!
//! - track the current [`LifecycleState`]
//! - validate deterministic state transitions
//! - notify observers on every change, in subscription order
//! - unsubscribe automatically via RAII [`Subscription`] guards
//! - expose a manually controlled [`LifecycleRegistry`] useful for tests
//!   and for platform adapters (Android, Apple, Web) that forward native
//!   lifecycle events into the pure Rust core.
//!
//! The registry is single-threaded (`!Send`, `!Sync`) by design. It uses
//! `Rc`/`RefCell` rather than `Arc`/`Mutex` so it works naturally on
//! WebAssembly and in single-threaded UI runtimes. Multithreaded hosts can
//! wrap the registry in a `Mutex` or confine it to one thread.
//!
//! Currently requires `std`; only `alloc`/`core` features are used for the
//! retained data, so future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
//!
//! let lifecycle = LifecycleRegistry::new();
//! let _guard = lifecycle.subscribe(|state| {
//!     println!("lifecycle moved to {state:?}");
//! });
//!
//! lifecycle.create().unwrap();
//! lifecycle.start().unwrap();
//! lifecycle.resume().unwrap();
//! assert_eq!(lifecycle.state(), LifecycleState::Resumed);
//! ```

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use thiserror::Error;

/// Ordered lifecycle states.
///
/// The ordering (excluding [`LifecycleState::Destroyed`], which is terminal)
/// is `Initialized < Created < Started < Resumed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleState {
    /// Freshly constructed registry; no lifecycle event delivered yet.
    Initialized,
    /// Component created (e.g. `onCreate`).
    Created,
    /// Component started and visible (e.g. `onStart`).
    Started,
    /// Component resumed and interactive (e.g. `onResume`).
    Resumed,
    /// Terminal state. No further transitions are allowed.
    Destroyed,
}

impl LifecycleState {
    /// Returns `true` for [`LifecycleState::Resumed`].
    #[must_use]
    pub fn is_resumed(self) -> bool {
        matches!(self, Self::Resumed)
    }

    /// Returns `true` for [`LifecycleState::Started`] or
    /// [`LifecycleState::Resumed`].
    #[must_use]
    pub fn is_started(self) -> bool {
        matches!(self, Self::Started | Self::Resumed)
    }

    /// Returns `true` for the terminal [`LifecycleState::Destroyed`] state.
    #[must_use]
    pub fn is_destroyed(self) -> bool {
        matches!(self, Self::Destroyed)
    }

    /// Rank along the forward chain, used to walk intermediate states.
    fn rank(self) -> u8 {
        match self {
            Self::Initialized => 0,
            Self::Created => 1,
            Self::Started => 2,
            Self::Resumed => 3,
            Self::Destroyed => 4,
        }
    }

    /// State for a given non-terminal rank.
    fn from_rank(rank: u8) -> Self {
        match rank {
            0 => Self::Initialized,
            1 => Self::Created,
            2 => Self::Started,
            _ => Self::Resumed,
        }
    }
}

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

/// RAII subscription guard.
///
/// Dropping the guard automatically unsubscribes the observer. Use
/// [`Subscription::unsubscribe`] for explicit early unsubscription, or
/// [`Subscription::detach`] to keep the observer registered after the guard
/// is dropped.
#[derive(Debug)]
pub struct Subscription {
    inner: Option<SubscriptionInner>,
}

#[derive(Debug)]
struct SubscriptionInner {
    registry: Weak<RefCell<RegistryInner>>,
    id: u64,
}

impl Subscription {
    fn new(registry: &Rc<RefCell<RegistryInner>>, id: u64) -> Self {
        Self { inner: Some(SubscriptionInner { registry: Rc::downgrade(registry), id }) }
    }

    /// Returns `true` while the observer is still registered.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.inner.as_ref().is_some_and(|guard| {
            guard.registry.upgrade().is_some_and(|registry| {
                registry.borrow().observers.iter().any(|(id, _)| *id == guard.id)
            })
        })
    }

    /// Explicitly unsubscribes the observer. Dropping without detaching
    /// unsubscribes as well; calling this merely makes it explicit.
    pub fn unsubscribe(mut self) {
        self.remove();
    }

    /// Detaches the observer so it stays registered after this guard is
    /// dropped. The observer can then only be removed via
    /// [`LifecycleRegistry::unsubscribe`] with the returned id, or by
    /// destroying the registry.
    #[must_use]
    pub fn detach(mut self) -> u64 {
        self.inner.take().map_or(u64::MAX, |guard| guard.id)
    }

    fn remove(&mut self) {
        if let Some(guard) = self.inner.take() {
            if let Some(registry) = guard.registry.upgrade() {
                registry.borrow_mut().observers.retain(|(id, _)| *id != guard.id);
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.remove();
    }
}

type Observer = Rc<dyn Fn(LifecycleState)>;

struct RegistryInner {
    state: LifecycleState,
    observers: Vec<(u64, Observer)>,
    next_id: u64,
}

impl std::fmt::Debug for RegistryInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Observer closures intentionally omitted: `dyn Fn` has no `Debug`.
        f.debug_struct("RegistryInner")
            .field("state", &self.state)
            .field("observer_count", &self.observers.len())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

impl RegistryInner {
    fn new() -> Self {
        Self { state: LifecycleState::Initialized, observers: Vec::new(), next_id: 0 }
    }
}

impl Default for RegistryInner {
    fn default() -> Self {
        Self::new()
    }
}

/// Manually controlled lifecycle registry.
///
/// Cheaply [`Clone`]able: clones share the same underlying state and observer
/// list, which is convenient for platform adapters that hand out handles.
///
/// Notifications are delivered synchronously, in subscription order, after the
/// state has been updated. The observer list is snapshotted before delivery,
/// so observers may safely subscribe, unsubscribe, or trigger further
/// transitions from inside a callback (nested transitions notify
/// synchronously).
#[derive(Debug, Clone, Default)]
pub struct LifecycleRegistry {
    inner: Rc<RefCell<RegistryInner>>,
}

impl LifecycleRegistry {
    /// Creates a registry in [`LifecycleState::Initialized`].
    #[must_use]
    pub fn new() -> Self {
        Self { inner: Rc::new(RefCell::new(RegistryInner::new())) }
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> LifecycleState {
        self.inner.borrow().state
    }

    /// Returns the number of currently registered observers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.inner.borrow().observers.len()
    }

    /// Subscribes an observer. The observer is called with the new state on
    /// every subsequent transition, in subscription order.
    ///
    /// The returned [`Subscription`] unsubscribes on drop.
    pub fn subscribe(&self, observer: impl Fn(LifecycleState) + 'static) -> Subscription {
        let observer: Observer = Rc::new(observer);
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.observers.push((id, observer));
        Subscription::new(&self.inner, id)
    }

    /// Removes the observer registered under `id`. Returns `true` if an
    /// observer was removed.
    #[must_use]
    pub fn unsubscribe(&self, id: u64) -> bool {
        let mut inner = self.inner.borrow_mut();
        let before = inner.observers.len();
        inner.observers.retain(|(other, _)| *other != id);
        inner.observers.len() != before
    }

    /// Moves toward `target`, delivering each intermediate state in order.
    ///
    /// Moving to [`LifecycleState::Destroyed`] from any non-destroyed state
    /// is a single terminal transition. Moving between the ordered states
    /// walks step by step (for example `Initialized -> Resumed` delivers
    /// `Created`, `Started`, then `Resumed`). Requesting the current state
    /// is a no-op that delivers nothing.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::AlreadyDestroyed`] if the registry is
    /// destroyed, including a request to move to `Destroyed` again.
    pub fn move_to(&self, target: LifecycleState) -> Result<(), LifecycleError> {
        let from = self.inner.borrow().state;
        if from.is_destroyed() {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        if target.is_destroyed() {
            if from.is_destroyed() {
                return Err(LifecycleError::AlreadyDestroyed);
            }
            return self.apply(target);
        }
        if from == target {
            return Ok(());
        }
        let (mut rank, target_rank) = (from.rank(), target.rank());
        if rank < target_rank {
            while rank < target_rank {
                rank += 1;
                self.apply(LifecycleState::from_rank(rank))?;
            }
        } else {
            while rank > target_rank {
                rank -= 1;
                self.apply(LifecycleState::from_rank(rank))?;
            }
        }
        Ok(())
    }

    /// `Initialized -> Created`.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] unless currently
    /// `Initialized`.
    pub fn create(&self) -> Result<(), LifecycleError> {
        self.step(LifecycleState::Initialized, LifecycleState::Created)
    }

    /// `Created -> Started`.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] unless currently
    /// `Created`.
    pub fn start(&self) -> Result<(), LifecycleError> {
        self.step(LifecycleState::Created, LifecycleState::Started)
    }

    /// `Started -> Resumed`.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] unless currently
    /// `Started`.
    pub fn resume(&self) -> Result<(), LifecycleError> {
        self.step(LifecycleState::Started, LifecycleState::Resumed)
    }

    /// `Resumed -> Started`.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] unless currently
    /// `Resumed`.
    pub fn pause(&self) -> Result<(), LifecycleError> {
        self.step(LifecycleState::Resumed, LifecycleState::Started)
    }

    /// `Started -> Created`.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::InvalidTransition`] unless currently
    /// `Started`.
    pub fn stop(&self) -> Result<(), LifecycleError> {
        self.step(LifecycleState::Started, LifecycleState::Created)
    }

    /// Any non-destroyed state `-> Destroyed`. Terminal.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::AlreadyDestroyed`] if already destroyed.
    pub fn destroy(&self) -> Result<(), LifecycleError> {
        let from = self.inner.borrow().state;
        if from.is_destroyed() {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        self.apply(LifecycleState::Destroyed)
    }

    fn step(&self, from: LifecycleState, to: LifecycleState) -> Result<(), LifecycleError> {
        let current = self.inner.borrow().state;
        if current.is_destroyed() {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        if current != from {
            return Err(LifecycleError::InvalidTransition { from: current, to });
        }
        self.apply(to)
    }

    fn apply(&self, next: LifecycleState) -> Result<(), LifecycleError> {
        let observers = {
            let mut inner = self.inner.borrow_mut();
            if inner.state.is_destroyed() {
                return Err(LifecycleError::AlreadyDestroyed);
            }
            inner.state = next;
            inner.observers.iter().map(|(_, o)| Rc::clone(o)).collect::<Vec<_>>()
        };
        for observer in observers {
            observer(next);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell as StdRefCell;
    use std::rc::Rc as StdRc;

    #[test]
    fn initial_state_is_initialized() {
        let lifecycle = LifecycleRegistry::new();
        assert_eq!(lifecycle.state(), LifecycleState::Initialized);
        assert_eq!(lifecycle.subscriber_count(), 0);
    }

    #[test]
    fn full_forward_and_backward_cycle() {
        let lifecycle = LifecycleRegistry::new();
        lifecycle.create().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Created);
        lifecycle.start().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Started);
        lifecycle.resume().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Resumed);
        lifecycle.pause().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Started);
        lifecycle.stop().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Created);
        lifecycle.destroy().unwrap();
        assert_eq!(lifecycle.state(), LifecycleState::Destroyed);
    }

    #[test]
    fn invalid_strict_steps_are_rejected() {
        let lifecycle = LifecycleRegistry::new();
        assert_eq!(
            lifecycle.start().unwrap_err(),
            LifecycleError::InvalidTransition {
                from: LifecycleState::Initialized,
                to: LifecycleState::Started,
            }
        );
        assert_eq!(
            lifecycle.resume().unwrap_err(),
            LifecycleError::InvalidTransition {
                from: LifecycleState::Initialized,
                to: LifecycleState::Resumed,
            }
        );
        assert_eq!(
            lifecycle.pause().unwrap_err(),
            LifecycleError::InvalidTransition {
                from: LifecycleState::Initialized,
                to: LifecycleState::Started,
            }
        );
        lifecycle.create().unwrap();
        assert!(lifecycle.resume().is_err());
        assert!(lifecycle.pause().is_err());
    }

    #[test]
    fn destroyed_is_terminal() {
        let lifecycle = LifecycleRegistry::new();
        lifecycle.destroy().unwrap();
        assert_eq!(lifecycle.destroy().unwrap_err(), LifecycleError::AlreadyDestroyed);
        assert_eq!(
            lifecycle.move_to(LifecycleState::Created).unwrap_err(),
            LifecycleError::AlreadyDestroyed
        );
        assert_eq!(lifecycle.create().unwrap_err(), LifecycleError::AlreadyDestroyed);
    }

    #[test]
    fn move_to_walks_intermediate_states_in_order() {
        let lifecycle = LifecycleRegistry::new();
        let seen = StdRc::new(StdRefCell::new(Vec::new()));
        let probe = StdRc::clone(&seen);
        let _guard = lifecycle.subscribe(move |state| probe.borrow_mut().push(state));
        lifecycle.move_to(LifecycleState::Resumed).unwrap();
        assert_eq!(
            *seen.borrow(),
            vec![LifecycleState::Created, LifecycleState::Started, LifecycleState::Resumed,]
        );
        lifecycle.move_to(LifecycleState::Created).unwrap();
        assert_eq!(
            *seen.borrow(),
            vec![
                LifecycleState::Created,
                LifecycleState::Started,
                LifecycleState::Resumed,
                LifecycleState::Started,
                LifecycleState::Created,
            ]
        );
    }

    #[test]
    fn move_to_current_state_is_noop() {
        let lifecycle = LifecycleRegistry::new();
        let calls = StdRc::new(StdRefCell::new(0_u32));
        let probe = StdRc::clone(&calls);
        let _guard = lifecycle.subscribe(move |_| *probe.borrow_mut() += 1);
        lifecycle.move_to(LifecycleState::Initialized).unwrap();
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn observers_fire_in_subscription_order() {
        let lifecycle = LifecycleRegistry::new();
        let log = StdRc::new(StdRefCell::new(Vec::new()));
        let a = StdRc::clone(&log);
        let b = StdRc::clone(&log);
        let _ga = lifecycle.subscribe(move |_| a.borrow_mut().push("a"));
        let _gb = lifecycle.subscribe(move |_| b.borrow_mut().push("b"));
        lifecycle.create().unwrap();
        assert_eq!(*log.borrow(), vec!["a", "b"]);
    }

    #[test]
    fn dropping_subscription_unsubscribes() {
        let lifecycle = LifecycleRegistry::new();
        let calls = StdRc::new(StdRefCell::new(0_u32));
        let probe = StdRc::clone(&calls);
        let guard = lifecycle.subscribe(move |_| *probe.borrow_mut() += 1);
        assert_eq!(lifecycle.subscriber_count(), 1);
        assert!(guard.is_active());
        drop(guard);
        assert_eq!(lifecycle.subscriber_count(), 0);
        lifecycle.create().unwrap();
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn registry_clone_shares_state() {
        let lifecycle = LifecycleRegistry::new();
        let other = lifecycle.clone();
        lifecycle.create().unwrap();
        assert_eq!(other.state(), LifecycleState::Created);
    }

    #[test]
    fn reentrant_subscribe_from_callback_is_safe() {
        let lifecycle = LifecycleRegistry::new();
        let lifecycle_ref = lifecycle.clone();
        let fired = StdRc::new(StdRefCell::new(Vec::new()));
        let probe = StdRc::clone(&fired);
        let _guard = lifecycle.subscribe(move |state| {
            probe.borrow_mut().push(state);
            if state == LifecycleState::Created {
                let probe2 = StdRc::clone(&probe);
                // Detach so the new observer outlives this callback frame.
                let _ = lifecycle_ref.subscribe(move |s| probe2.borrow_mut().push(s)).detach();
            }
        });
        lifecycle.create().unwrap();
        lifecycle.start().unwrap();
        // First observer fires twice; second observer (added mid-flight)
        // fires once for `Started`.
        assert_eq!(fired.borrow().len(), 3);
    }

    #[test]
    fn destroy_notifies_observers_once() {
        let lifecycle = LifecycleRegistry::new();
        lifecycle.move_to(LifecycleState::Resumed).unwrap();
        let seen = StdRc::new(StdRefCell::new(Vec::new()));
        let probe = StdRc::clone(&seen);
        let _guard = lifecycle.subscribe(move |state| probe.borrow_mut().push(state));
        lifecycle.destroy().unwrap();
        assert_eq!(*seen.borrow(), vec![LifecycleState::Destroyed]);
    }
}
