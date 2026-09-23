use essenty_back_handler::BackDispatcher;

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
