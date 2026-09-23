use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Event, Window, window};

use crate::HistoryBackBridge;

use super::BrowserError;

/// Observes browser `popstate` events and dispatches them to Rust callbacks.
///
/// The browser has already navigated when `popstate` fires. A callback result
/// cannot prevent that navigation. The listener is removed on drop.
pub struct BrowserHistoryBack {
    bridge: Rc<RefCell<HistoryBackBridge>>,
    window: Window,
    listener: Closure<dyn FnMut(Event)>,
}

impl std::fmt::Debug for BrowserHistoryBack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserHistoryBack").finish_non_exhaustive()
    }
}

impl BrowserHistoryBack {
    /// Registers a `popstate` listener on the current window.
    ///
    /// # Errors
    /// Returns an error when the window is unavailable or registration fails.
    pub fn new() -> Result<Self, BrowserError> {
        let window = window().ok_or(BrowserError::Unavailable)?;
        let bridge = Rc::new(RefCell::new(HistoryBackBridge::new()));
        let callback_bridge = Rc::clone(&bridge);
        let listener = Closure::<dyn FnMut(Event)>::new(move |_| {
            callback_bridge.borrow_mut().on_pop_state();
        });
        window
            .add_event_listener_with_callback("popstate", listener.as_ref().unchecked_ref())
            .map_err(|_| BrowserError::Listener)?;
        Ok(Self { bridge, window, listener })
    }

    /// Shared bridge for registering Rust back callbacks.
    #[must_use]
    pub fn bridge(&self) -> Rc<RefCell<HistoryBackBridge>> {
        Rc::clone(&self.bridge)
    }
}

impl Drop for BrowserHistoryBack {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback(
            "popstate",
            self.listener.as_ref().unchecked_ref(),
        );
    }
}
