use crate::{BackError, BackEvent, GesturePosition};
use std::collections::BTreeMap;

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
            return false;
        };
        self.gesture_in_flight = true;
        self.active_gesture = Some(id);
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
            if let Some(id) = self.active_gesture {
                self.dispatch_to(id, BackEvent::started(), true);
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
