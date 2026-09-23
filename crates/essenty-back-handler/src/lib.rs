//! Pure Rust back-event dispatcher.
//!
//! Handles both ordinary back presses and the generalized predictive
//! (progress-based) back gesture model:
//!
//! - callbacks register with a priority and an enabled flag
//! - the highest-priority enabled handler wins; ties break toward the most
//!   recently registered callback
//! - [`BackDispatcher::back`] performs a regular back invocation
//! - [`BackDispatcher::predictive_start`] claims the gesture for the current
//!   winner; [`BackDispatcher::predictive_progress`],
//!   [`BackDispatcher::predictive_cancel`], and
//!   [`BackDispatcher::predictive_invoke`] route to that claimed handler so
//!   the gesture stays coherent even if priorities change mid-gesture;
//!   removal cancels the owner and lets a later progress event claim a fallback
//!
//! No Android (or any platform) API appears here; platform crates forward
//! native back events into this dispatcher.
//!
//! The dispatcher is single-threaded (`!Send`, `!Sync`) and synchronous. It
//! currently requires `std`, but only `alloc`/`core` containers are used, so
//! future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_back_handler::BackDispatcher;
//!
//! let mut dispatcher = BackDispatcher::new();
//! let handle = dispatcher.register(0, true, |event| {
//!     println!("back: {event:?}");
//! });
//! assert!(dispatcher.back());
//! dispatcher.unregister(handle.id());
//! ```

use std::collections::BTreeMap;

use thiserror::Error;

/// Phase of a back event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackPhase {
    /// Predictive gesture started.
    Started,
    /// Predictive gesture progressed (carries progress in the event).
    Progressed,
    /// Predictive gesture cancelled.
    Cancelled,
    /// Back invoked (regular press or predictive completion).
    Invoked,
}

/// Back event delivered to callbacks.
///
/// Progress (`0.0..=1.0` by convention) is present only for
/// [`BackPhase::Progressed`]; all other phases carry `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackEvent {
    /// Which phase this event represents.
    pub phase: BackPhase,
    /// Gesture progress; `Some` only for `Progressed`.
    pub progress: Option<f32>,
}

impl BackEvent {
    /// Regular back invocation (e.g. hardware back button).
    #[must_use]
    pub fn invoked() -> Self {
        Self { phase: BackPhase::Invoked, progress: None }
    }

    /// Predictive gesture started.
    #[must_use]
    pub fn started() -> Self {
        Self { phase: BackPhase::Started, progress: None }
    }

    /// Predictive gesture progressed. `progress` is conventionally
    /// `0.0..=1.0`; values are stored as given.
    #[must_use]
    pub fn progressed(progress: f32) -> Self {
        Self { phase: BackPhase::Progressed, progress: Some(progress) }
    }

    /// Predictive gesture cancelled.
    #[must_use]
    pub fn cancelled() -> Self {
        Self { phase: BackPhase::Cancelled, progress: None }
    }
}

/// Errors for invalid predictive-gesture sequences.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BackError {
    /// A progress/cancel/invoke arrived with no gesture in flight.
    #[error("no predictive back gesture in progress")]
    NoGestureInProgress,
}

/// Registration token returned by [`BackDispatcher::register`].
///
/// This is a plain id holder (no `Drop` removal): call
/// [`BackDispatcher::unregister`] with [`BackHandle::id`] to remove the
/// callback, or [`BackDispatcher::set_enabled`] to toggle it. Explicit ids
/// keep reentrancy semantics obvious — a callback may unregister any id,
/// including its own, without aliasing hazards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BackHandle {
    id: u64,
}

impl BackHandle {
    /// Registration id for explicit management.
    #[must_use]
    pub fn id(self) -> u64 {
        self.id
    }
}

type Callback = Box<dyn FnMut(BackEvent, &mut BackCommands<'_>)>;

enum BackCommand {
    Register(u64, i32, bool, Callback),
    Unregister(u64),
    SetEnabled(u64, bool),
}

/// Changes requested by a callback while an event is being delivered.
/// They take effect immediately after that callback returns, avoiding a
/// mutable alias of the dispatcher during callback execution.
pub struct BackCommands<'a> {
    next_id: &'a mut u64,
    pending: Vec<BackCommand>,
}

impl std::fmt::Debug for BackCommands<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackCommands").field("pending_count", &self.pending.len()).finish()
    }
}

impl BackCommands<'_> {
    /// Queues a new handler and returns its id.
    pub fn register(
        &mut self,
        priority: i32,
        enabled: bool,
        callback: impl FnMut(BackEvent, &mut BackCommands<'_>) + 'static,
    ) -> BackHandle {
        let id = *self.next_id;
        *self.next_id += 1;
        self.pending.push(BackCommand::Register(id, priority, enabled, Box::new(callback)));
        BackHandle { id }
    }

    /// Queues removal of a handler, including the currently executing one.
    pub fn unregister(&mut self, id: u64) {
        self.pending.push(BackCommand::Unregister(id));
    }

    /// Queues an enabled-state change.
    pub fn set_enabled(&mut self, id: u64, enabled: bool) {
        self.pending.push(BackCommand::SetEnabled(id, enabled));
    }
}

struct Entry {
    priority: i32,
    enabled: bool,
    callback: Callback,
}

/// Back-event dispatcher.
///
/// Registration order plus priority decide the winner: the enabled callback
/// with the highest `(priority, registration id)` pair handles the event. A
/// predictive gesture is claimed at
/// [`BackDispatcher::predictive_start`] time and subsequent gesture events
/// route to the claimed handler.
#[derive(Default)]
pub struct BackDispatcher {
    entries: BTreeMap<u64, Entry>,
    next_id: u64,
    active_gesture: Option<u64>,
    gesture_in_flight: bool,
}

impl std::fmt::Debug for BackDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Callbacks intentionally omitted: `dyn FnMut` has no `Debug`.
        f.debug_struct("BackDispatcher")
            .field("handler_count", &self.entries.len())
            .field("active_gesture", &self.active_gesture)
            .finish_non_exhaustive()
    }
}

impl BackDispatcher {
    /// Creates an empty dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered callbacks (enabled or not).
    #[must_use]
    pub fn handler_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when at least one enabled callback is registered.
    #[must_use]
    pub fn can_handle(&self) -> bool {
        self.entries.values().any(|entry| entry.enabled)
    }

    /// Returns `true` while a predictive gesture is in flight.
    #[must_use]
    pub fn has_active_gesture(&self) -> bool {
        self.gesture_in_flight
    }

    /// Registers a callback with a priority and initial enabled flag.
    ///
    /// Higher `priority` wins; ties break toward the later registration.
    pub fn register(
        &mut self,
        priority: i32,
        enabled: bool,
        callback: impl FnMut(BackEvent) + 'static,
    ) -> BackHandle {
        let mut callback = callback;
        self.register_reentrant(priority, enabled, move |event, _| callback(event))
    }

    /// Registers a callback that may queue registration changes during dispatch.
    pub fn register_reentrant(
        &mut self,
        priority: i32,
        enabled: bool,
        callback: impl FnMut(BackEvent, &mut BackCommands<'_>) + 'static,
    ) -> BackHandle {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.insert(id, Entry { priority, enabled, callback: Box::new(callback) });
        BackHandle { id }
    }

    /// Removes the callback registered under `id`. If it held the active
    /// gesture, it receives cancellation; a later progress event may choose
    /// another handler. Returns `true` if one existed.
    pub fn unregister(&mut self, id: u64) -> bool {
        if self.active_gesture == Some(id) {
            self.active_gesture = None;
            self.dispatch_to(id, BackEvent::cancelled(), true);
        }
        self.entries.remove(&id).is_some()
    }

    /// Enables or disables a callback. Returns `true` if `id` exists.
    /// Disabling a gesture owner affects future gestures but does not change
    /// the owner already claimed for the current gesture.
    pub fn set_enabled(&mut self, id: u64, enabled: bool) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Returns `true` when `id` is registered and enabled.
    #[must_use]
    pub fn is_enabled(&self, id: u64) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.enabled)
    }

    /// Regular back invocation. Routes to the current winner.
    /// Returns `true` when a handler consumed the event.
    pub fn back(&mut self) -> bool {
        let id = self.active_gesture.take().or_else(|| self.winner());
        self.gesture_in_flight = false;
        let Some(id) = id else {
            return false;
        };
        self.dispatch_to(id, BackEvent::invoked(), true)
    }

    /// Starts a predictive gesture, claiming the current winner.
    /// Returns `true` when a handler consumed the start.
    pub fn predictive_start(&mut self) -> bool {
        let Some(id) = self.winner() else {
            self.active_gesture = None;
            self.gesture_in_flight = false;
            return false;
        };
        self.gesture_in_flight = true;
        self.active_gesture = Some(id);
        self.dispatch_to(id, BackEvent::started(), true)
    }

    /// Delivers progress to the gesture owner.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`] when no gesture is in
    /// flight.
    pub fn predictive_progress(&mut self, progress: f32) -> Result<bool, BackError> {
        if !self.gesture_in_flight {
            return Err(BackError::NoGestureInProgress);
        }
        if self.active_gesture.is_none() {
            self.active_gesture = self.winner();
            if let Some(id) = self.active_gesture {
                self.dispatch_to(id, BackEvent::started(), true);
            }
        }
        Ok(self
            .active_gesture
            .is_some_and(|id| self.dispatch_to(id, BackEvent::progressed(progress), true)))
    }

    /// Cancels the in-flight gesture, delivering `Cancelled` to its owner.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`] when no gesture is in
    /// flight.
    pub fn predictive_cancel(&mut self) -> Result<bool, BackError> {
        if !self.gesture_in_flight {
            return Err(BackError::NoGestureInProgress);
        }
        self.gesture_in_flight = false;
        Ok(self
            .active_gesture
            .take()
            .is_some_and(|id| self.dispatch_to(id, BackEvent::cancelled(), true)))
    }

    /// Completes the in-flight gesture, delivering `Invoked` to its owner.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`] when no gesture is in
    /// flight.
    pub fn predictive_invoke(&mut self) -> Result<bool, BackError> {
        if !self.gesture_in_flight {
            return Err(BackError::NoGestureInProgress);
        }
        self.gesture_in_flight = false;
        let id = self.active_gesture.take().or_else(|| self.winner());
        Ok(id.is_some_and(|id| self.dispatch_to(id, BackEvent::invoked(), true)))
    }

    fn winner(&self) -> Option<u64> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.enabled)
            .max_by_key(|(id, entry)| (entry.priority, *id))
            .map(|(id, _)| *id)
    }

    fn dispatch_to(&mut self, id: u64, event: BackEvent, force: bool) -> bool {
        let mut commands = BackCommands { next_id: &mut self.next_id, pending: Vec::new() };
        let mut handled = false;
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry.enabled || force {
                (entry.callback)(event, &mut commands);
                handled = true;
            }
        }
        for command in commands.pending {
            match command {
                BackCommand::Register(id, priority, enabled, callback) => {
                    self.entries.insert(id, Entry { priority, enabled, callback });
                }
                BackCommand::Unregister(id) => {
                    self.unregister(id);
                }
                BackCommand::SetEnabled(id, enabled) => {
                    self.set_enabled(id, enabled);
                }
            }
        }
        handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn no_handler_returns_false() {
        let mut dispatcher = BackDispatcher::new();
        assert!(!dispatcher.can_handle());
        assert!(!dispatcher.back());
        assert!(!dispatcher.predictive_start());
        assert_eq!(
            dispatcher.predictive_progress(0.5).unwrap_err(),
            BackError::NoGestureInProgress
        );
        assert_eq!(dispatcher.predictive_cancel().unwrap_err(), BackError::NoGestureInProgress);
        assert_eq!(dispatcher.predictive_invoke().unwrap_err(), BackError::NoGestureInProgress);
    }

    #[test]
    fn single_handler_receives_back() {
        let mut dispatcher = BackDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let probe = Rc::clone(&log);
        let handle = dispatcher.register(0, true, move |event| probe.borrow_mut().push(event));
        assert!(dispatcher.can_handle());
        assert!(dispatcher.back());
        assert_eq!(*log.borrow(), vec![BackEvent::invoked()]);
        assert!(dispatcher.unregister(handle.id()));
        assert!(!dispatcher.back());
    }

    #[test]
    fn disabled_handler_is_skipped() {
        let mut dispatcher = BackDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let probe = Rc::clone(&log);
        let handle = dispatcher.register(10, false, move |event| {
            probe.borrow_mut().push(event);
        });
        let id = handle.id();
        assert!(!dispatcher.back());
        assert!(log.borrow().is_empty());
        assert!(dispatcher.set_enabled(id, true));
        assert!(dispatcher.is_enabled(id));
        assert!(dispatcher.back());
        assert_eq!(log.borrow().len(), 1);
        dispatcher.set_enabled(id, false);
        assert!(!dispatcher.back());
    }

    #[test]
    fn highest_priority_wins() {
        let mut dispatcher = BackDispatcher::new();
        let low = Rc::new(RefCell::new(0_u32));
        let high = Rc::new(RefCell::new(0_u32));
        let low_probe = Rc::clone(&low);
        let high_probe = Rc::clone(&high);
        dispatcher.register(1, true, move |_| *low_probe.borrow_mut() += 1);
        dispatcher.register(5, true, move |_| *high_probe.borrow_mut() += 1);
        assert!(dispatcher.back());
        assert_eq!(*low.borrow(), 0);
        assert_eq!(*high.borrow(), 1);
    }

    #[test]
    fn later_registration_breaks_priority_ties() {
        let mut dispatcher = BackDispatcher::new();
        let first = Rc::new(RefCell::new(0_u32));
        let second = Rc::new(RefCell::new(0_u32));
        let a = Rc::clone(&first);
        let b = Rc::clone(&second);
        dispatcher.register(0, true, move |_| *a.borrow_mut() += 1);
        dispatcher.register(0, true, move |_| *b.borrow_mut() += 1);
        assert!(dispatcher.back());
        assert_eq!(*first.borrow(), 0);
        assert_eq!(*second.borrow(), 1);
    }

    #[test]
    fn unregister_by_id_stops_delivery() {
        let mut dispatcher = BackDispatcher::new();
        let log = Rc::new(RefCell::new(0_u32));
        let probe = Rc::clone(&log);
        let handle = dispatcher.register(0, true, move |_| *probe.borrow_mut() += 1);
        let id = handle.id();
        assert!(dispatcher.unregister(id));
        assert!(!dispatcher.unregister(id));
        assert!(!dispatcher.back());
        assert_eq!(*log.borrow(), 0);
    }

    #[test]
    fn predictive_gesture_lifecycle() {
        let mut dispatcher = BackDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let probe = Rc::clone(&log);
        let _handle = dispatcher.register(0, true, move |event| {
            probe.borrow_mut().push(event);
        });
        assert!(dispatcher.predictive_start());
        assert!(dispatcher.has_active_gesture());
        assert!(dispatcher.predictive_progress(0.25).unwrap());
        assert!(dispatcher.predictive_progress(0.75).unwrap());
        assert!(dispatcher.predictive_cancel().unwrap());
        assert!(!dispatcher.has_active_gesture());
        assert_eq!(
            *log.borrow(),
            vec![
                BackEvent::started(),
                BackEvent::progressed(0.25),
                BackEvent::progressed(0.75),
                BackEvent::cancelled(),
            ]
        );
    }

    #[test]
    fn predictive_invoke_completes_gesture() {
        let mut dispatcher = BackDispatcher::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let probe = Rc::clone(&log);
        let _handle = dispatcher.register(0, true, move |event| {
            probe.borrow_mut().push(event);
        });
        assert!(dispatcher.predictive_start());
        assert!(dispatcher.predictive_invoke().unwrap());
        assert!(!dispatcher.has_active_gesture());
        assert_eq!(*log.borrow(), vec![BackEvent::started(), BackEvent::invoked(),]);
        // Gesture is over; further gesture events error.
        assert_eq!(
            dispatcher.predictive_progress(0.5).unwrap_err(),
            BackError::NoGestureInProgress
        );
    }

    #[test]
    fn gesture_sticks_to_claimed_handler() {
        let mut dispatcher = BackDispatcher::new();
        let first = Rc::new(RefCell::new(Vec::new()));
        let a = Rc::clone(&first);
        dispatcher.register(1, true, move |event| a.borrow_mut().push(event));
        assert!(dispatcher.predictive_start());
        // A new higher-priority handler appears mid-gesture, but the
        // in-flight gesture still routes to the claimed owner.
        let newcomer_calls = Rc::new(RefCell::new(0_u32));
        let newcomer_probe = Rc::clone(&newcomer_calls);
        let _new_high = dispatcher.register(99, true, move |_| {
            *newcomer_probe.borrow_mut() += 1;
        });
        assert!(dispatcher.predictive_progress(0.5).unwrap());
        assert!(dispatcher.predictive_invoke().unwrap());
        assert_eq!(first.borrow().len(), 3);
        assert_eq!(*newcomer_calls.borrow(), 0);
        // After the gesture, the newcomer wins regular back handling.
        assert!(dispatcher.back());
        assert_eq!(*newcomer_calls.borrow(), 1);
    }

    #[test]
    fn callback_can_unregister_itself_and_register_successor() {
        use std::cell::Cell;
        let mut dispatcher = BackDispatcher::new();
        let own_id = Rc::new(Cell::new(u64::MAX));
        let own_id_in_callback = Rc::clone(&own_id);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let first_calls = Rc::clone(&calls);
        let successor_calls = Rc::clone(&calls);
        let handle = dispatcher.register_reentrant(0, true, move |event, commands| {
            if event.phase == BackPhase::Invoked {
                first_calls.borrow_mut().push("first");
                commands.unregister(own_id_in_callback.get());
                commands.register(0, true, {
                    let successor_calls = Rc::clone(&successor_calls);
                    move |_, _| successor_calls.borrow_mut().push("successor")
                });
            }
        });
        own_id.set(handle.id());
        assert!(dispatcher.back());
        assert_eq!(dispatcher.handler_count(), 1);
        assert!(dispatcher.back());
        assert_eq!(*calls.borrow(), vec!["first", "successor"]);
    }

    #[test]
    fn removed_gesture_owner_is_cancelled_and_fallback_starts_on_progress() {
        let mut dispatcher = BackDispatcher::new();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let older = Rc::clone(&calls);
        let selected = Rc::clone(&calls);
        dispatcher.register(0, true, move |event| older.borrow_mut().push(("older", event.phase)));
        let selected_handle = dispatcher.register(0, true, move |event| {
            selected.borrow_mut().push(("selected", event.phase));
        });
        assert!(dispatcher.predictive_start());
        assert!(dispatcher.unregister(selected_handle.id()));
        assert!(dispatcher.has_active_gesture());
        assert!(dispatcher.predictive_progress(0.4).unwrap());
        assert!(dispatcher.predictive_invoke().unwrap());
        assert_eq!(
            *calls.borrow(),
            vec![
                ("selected", BackPhase::Started),
                ("selected", BackPhase::Cancelled),
                ("older", BackPhase::Started),
                ("older", BackPhase::Progressed),
                ("older", BackPhase::Invoked),
            ]
        );
    }

    #[test]
    fn disabling_gesture_owner_does_not_interrupt_claim() {
        let mut dispatcher = BackDispatcher::new();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let probe = Rc::clone(&calls);
        let handle =
            dispatcher.register(0, true, move |event| probe.borrow_mut().push(event.phase));
        assert!(dispatcher.predictive_start());
        assert!(dispatcher.set_enabled(handle.id(), false));
        assert!(dispatcher.predictive_progress(0.4).unwrap());
        assert!(dispatcher.predictive_cancel().unwrap());
        assert_eq!(
            *calls.borrow(),
            vec![BackPhase::Started, BackPhase::Progressed, BackPhase::Cancelled]
        );
    }
}
