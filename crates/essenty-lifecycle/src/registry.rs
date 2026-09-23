use crate::{LifecycleError, LifecycleState};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

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
/// Notifications are delivered synchronously, in transition order, after the
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
        let state = inner.state;
        if state != LifecycleState::Destroyed {
            inner.observers.push((id, Rc::clone(&observer)));
        }
        drop(inner);
        // Essenty catches new subscribers up to the current state. Invoke
        // outside the RefCell borrow so replay can change the registry.
        for replay in [LifecycleState::Created, LifecycleState::Started, LifecycleState::Resumed] {
            if state >= replay && state != LifecycleState::Destroyed {
                observer(replay);
            }
        }
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
    /// Moving to [`LifecycleState::Destroyed`] first walks backward through
    /// `Started` and `Created` when necessary. Moving between the ordered states
    /// walks step by step (for example `Initialized -> Resumed` delivers
    /// `Created`, `Started`, then `Resumed`). Requesting the current state
    /// is a no-op that delivers nothing. Returning to `Initialized` is invalid.
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
            return self.destroy();
        }
        if from == target {
            return Ok(());
        }
        if target == LifecycleState::Initialized {
            return Err(LifecycleError::InvalidTransition { from, to: target });
        }
        loop {
            let current = self.state();
            if current == target {
                break;
            }
            if current.is_destroyed() {
                return Err(LifecycleError::AlreadyDestroyed);
            }
            let next_rank = if current.rank() < target.rank() {
                current.rank() + 1
            } else {
                current.rank() - 1
            };
            self.apply(LifecycleState::from_rank(next_rank))?;
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

    /// Move through `Started` and `Created` before entering `Destroyed`.
    /// `Initialized` can be destroyed directly. Terminal.
    ///
    /// # Errors
    ///
    /// Returns [`LifecycleError::AlreadyDestroyed`] if already destroyed.
    pub fn destroy(&self) -> Result<(), LifecycleError> {
        let from = self.inner.borrow().state;
        if from.is_destroyed() {
            return Err(LifecycleError::AlreadyDestroyed);
        }
        if from != LifecycleState::Initialized {
            self.move_to(LifecycleState::Created)?;
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
            let previous = inner.state;
            inner.state = next;
            let mut observers =
                inner.observers.iter().map(|(_, o)| Rc::clone(o)).collect::<Vec<_>>();
            if matches!(
                next,
                LifecycleState::Started | LifecycleState::Created | LifecycleState::Destroyed
            ) && matches!(
                (next, previous),
                (LifecycleState::Started, LifecycleState::Resumed)
                    | (LifecycleState::Created, LifecycleState::Started)
                    | (LifecycleState::Destroyed, _)
            ) {
                observers.reverse();
            }
            observers
        };
        for observer in observers {
            observer(next);
        }
        if next == LifecycleState::Destroyed {
            self.inner.borrow_mut().observers.clear();
        }
        Ok(())
    }
}
