use crate::{BackError, BackEvent, GesturePosition};
use std::collections::BTreeMap;
use std::rc::Rc;

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
    gesture_start_event: Option<BackEvent>,
    enabled_listeners: BTreeMap<u64, EnabledListener>,
    next_listener_id: u64,
    has_enabled: bool,
}

type EnabledListener = Rc<dyn Fn(bool)>;

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

    /// Adds a listener invoked with the aggregate enabled state whenever it
    /// changes (at least one enabled handler appears or the last one
    /// disappears/is removed).
    ///
    /// This mirrors upstream `BackDispatcher.addEnabledChangedListener` and
    /// lets platform adapters (e.g. Android `OnBackInvokedCallback`) track
    /// native registration without polling. Returns a token for
    /// [`BackDispatcher::remove_enabled_changed_listener`].
    pub fn add_enabled_changed_listener(&mut self, listener: impl Fn(bool) + 'static) -> u64 {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.enabled_listeners.insert(id, Rc::new(listener));
        id
    }

    /// Removes a listener added by
    /// [`BackDispatcher::add_enabled_changed_listener`]. Returns `true` when
    /// one existed.
    pub fn remove_enabled_changed_listener(&mut self, id: u64) -> bool {
        self.enabled_listeners.remove(&id).is_some()
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
        self.notify_enabled_changed();
        BackHandle { id }
    }

    /// Notifies aggregate enabled-state listeners when `can_handle` changed.
    /// Listeners are snapshotted before delivery so they may add or remove
    /// listeners without aliasing the dispatcher.
    fn notify_enabled_changed(&mut self) {
        let now = self.can_handle();
        if now == self.has_enabled {
            return;
        }
        self.has_enabled = now;
        let listeners = self.enabled_listeners.values().map(Rc::clone).collect::<Vec<_>>();
        for listener in listeners {
            listener(now);
        }
    }

    /// Removes the callback registered under `id`. If it held the active
    /// gesture, it receives cancellation; a later progress event may choose
    /// another handler. Returns `true` if one existed.
    pub fn unregister(&mut self, id: u64) -> bool {
        if self.active_gesture == Some(id) {
            self.active_gesture = None;
            self.dispatch_to(id, BackEvent::cancelled(), true);
        }
        let removed = self.entries.remove(&id).is_some();
        self.notify_enabled_changed();
        removed
    }

    /// Enables or disables a callback. Returns `true` if `id` exists.
    /// Disabling a gesture owner affects future gestures but does not change
    /// the owner already claimed for the current gesture.
    pub fn set_enabled(&mut self, id: u64, enabled: bool) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.enabled = enabled;
            self.notify_enabled_changed();
            true
        } else {
            false
        }
    }

    /// Changes a callback's priority, affecting future winner selection but
    /// not the owner already claimed for the current gesture.
    ///
    /// This mirrors the mutable upstream `BackCallback.priority`.
    /// Returns `true` if `id` exists.
    pub fn set_priority(&mut self, id: u64, priority: i32) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.priority = priority;
            true
        } else {
            false
        }
    }

    /// Returns the priority of the callback registered under `id`, or `None`
    /// when `id` is unknown.
    #[must_use]
    pub fn priority(&self, id: u64) -> Option<i32> {
        self.entries.get(&id).map(|entry| entry.priority)
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
        self.gesture_start_event = None;
        let Some(id) = id else {
            return false;
        };
        self.dispatch_to(id, BackEvent::invoked(), true)
    }

    /// Starts a predictive gesture, claiming the current winner.
    /// Returns `true` when a handler consumed the start.
    pub fn predictive_start(&mut self) -> bool {
        self.start_gesture(BackEvent::started())
    }

    /// Starts a predictive gesture with edge and touch coordinates.
    pub fn predictive_start_with(&mut self, position: GesturePosition) -> bool {
        self.start_gesture(BackEvent::started_with(position))
    }

    fn start_gesture(&mut self, event: BackEvent) -> bool {
        let Some(id) = self.winner() else {
            self.active_gesture = None;
            self.gesture_in_flight = false;
            self.gesture_start_event = None;
            return false;
        };
        self.gesture_in_flight = true;
        self.active_gesture = Some(id);
        self.gesture_start_event = Some(event);
        self.dispatch_to(id, event, true)
    }

    /// Delivers progress to the gesture owner.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`] when no gesture is in
    /// flight.
    pub fn predictive_progress(&mut self, progress: f32) -> Result<bool, BackError> {
        self.progress_gesture(BackEvent::progressed(progress))
    }

    /// Delivers progress with edge and touch coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`] when no gesture is in flight.
    pub fn predictive_progress_with(
        &mut self,
        progress: f32,
        position: GesturePosition,
    ) -> Result<bool, BackError> {
        self.progress_gesture(BackEvent::progressed_with(progress, position))
    }

    fn progress_gesture(&mut self, event: BackEvent) -> Result<bool, BackError> {
        if !self.gesture_in_flight {
            return Err(BackError::NoGestureInProgress);
        }
        if self.active_gesture.is_none() {
            self.active_gesture = self.winner();
            // A fallback handler selected after the original owner was
            // removed first receives the original start event, mirroring
            // upstream `progressPredictiveBack` fallback behavior.
            if let (Some(id), Some(start)) = (self.active_gesture, self.gesture_start_event) {
                self.dispatch_to(id, start, true);
            }
        }
        Ok(self.active_gesture.is_some_and(|id| self.dispatch_to(id, event, true)))
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
        self.gesture_start_event = None;
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
        self.gesture_start_event = None;
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
        // Queued registrations bypass the direct `register` path; reconcile
        // aggregate listeners once after all commands applied.
        self.notify_enabled_changed();
        handled
    }
}
