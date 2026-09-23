use essenty_back_handler::BackDispatcher;

/// Forwards History API `popstate` events into a core [`BackDispatcher`].
///
/// `popstate` fires after the active history entry changes. Returning `true`
/// means a Rust callback ran; it cannot cancel browser navigation.
#[derive(Debug, Default)]
pub struct HistoryBackBridge {
    dispatcher: BackDispatcher,
}

impl HistoryBackBridge {
    /// Creates an empty bridge.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the dispatcher mutably for registration.
    #[must_use]
    pub fn dispatcher_mut(&mut self) -> &mut BackDispatcher {
        &mut self.dispatcher
    }

    /// Handles a `popstate` event.
    pub fn on_pop_state(&mut self) -> bool {
        self.dispatcher.back()
    }
}
