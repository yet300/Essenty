use essenty_back_handler::{BackDispatcher, GesturePosition};

/// Host-testable event mapping into the core [`BackDispatcher`]. The
/// Android-only `AndroidBackHandler` owns direct
/// platform callback registration.
///
/// The `NativeActivity` proxy adapter and host tests both use the same mapping
/// methods. This type itself remains platform agnostic and host-testable.
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

    #[cfg(all(target_os = "android", feature = "native-activity"))]
    pub(crate) fn with_dispatcher(dispatcher: BackDispatcher) -> Self {
        Self { dispatcher }
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

    /// Ordinary platform back equivalent.
    pub fn handle_back_pressed(&mut self) -> bool {
        self.dispatcher.back()
    }

    /// Predictive Back `onBackStarted` equivalent.
    pub fn handle_gesture_start(&mut self) -> bool {
        self.dispatcher.predictive_start()
    }

    /// Forwards predictive start with edge and touch coordinates.
    pub fn handle_gesture_start_with(&mut self, position: GesturePosition) -> bool {
        self.dispatcher.predictive_start_with(position)
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

    /// Forwards predictive progress with edge and touch coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`BackError::NoGestureInProgress`](essenty_back_handler::BackError)
    /// when no gesture is in flight.
    pub fn handle_gesture_progress_with(
        &mut self,
        progress: f32,
        position: GesturePosition,
    ) -> Result<bool, essenty_back_handler::BackError> {
        self.dispatcher.predictive_progress_with(progress, position)
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
