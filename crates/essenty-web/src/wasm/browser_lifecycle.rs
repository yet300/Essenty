use crate::VisibilityLifecycle;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{Document, Event, Window, window};

/// Error attaching browser event listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserError {
    /// No window or document is available.
    Unavailable,
    /// A listener could not be installed.
    Listener,
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("browser window or document unavailable"),
            Self::Listener => f.write_str("browser event listener registration failed"),
        }
    }
}

impl std::error::Error for BrowserError {}

fn read_visibility(document: &Document) -> Option<String> {
    js_sys::Reflect::get(document, &JsValue::from_str("visibilityState")).ok()?.as_string()
}

/// Observes browser visibility and page transitions until dropped.
pub struct BrowserLifecycle {
    lifecycle: VisibilityLifecycle,
    document: Document,
    window: Window,
    visibility: Closure<dyn FnMut(Event)>,
    page_hide: Closure<dyn FnMut(Event)>,
    page_show: Closure<dyn FnMut(Event)>,
}

impl std::fmt::Debug for BrowserLifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserLifecycle")
            .field("state", &self.lifecycle.registry().state())
            .finish_non_exhaustive()
    }
}

impl BrowserLifecycle {
    /// Attaches to the current document and reads its initial visibility.
    ///
    /// # Errors
    /// Returns an error if the DOM is unavailable or listener setup fails.
    pub fn new() -> Result<Self, BrowserError> {
        let window = window().ok_or(BrowserError::Unavailable)?;
        let document = window.document().ok_or(BrowserError::Unavailable)?;
        let lifecycle = VisibilityLifecycle::new();
        if let Some(state) = read_visibility(&document) {
            lifecycle.on_visibility_str(&state);
        }

        let document_for_visibility = document.clone();
        let visibility_lifecycle = lifecycle.clone();
        let visibility = Closure::<dyn FnMut(Event)>::new(move |_| {
            if let Some(state) = read_visibility(&document_for_visibility) {
                visibility_lifecycle.on_visibility_str(&state);
            }
        });
        document
            .add_event_listener_with_callback(
                "visibilitychange",
                visibility.as_ref().unchecked_ref(),
            )
            .map_err(|_| BrowserError::Listener)?;

        let hide_lifecycle = lifecycle.clone();
        let page_hide = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let persisted = event
                .dyn_ref::<web_sys::PageTransitionEvent>()
                .is_some_and(web_sys::PageTransitionEvent::persisted);
            hide_lifecycle.on_page_hide_with_persistence(persisted);
        });
        if window
            .add_event_listener_with_callback("pagehide", page_hide.as_ref().unchecked_ref())
            .is_err()
        {
            let _ = document.remove_event_listener_with_callback(
                "visibilitychange",
                visibility.as_ref().unchecked_ref(),
            );
            return Err(BrowserError::Listener);
        }

        let show_lifecycle = lifecycle.clone();
        let document_for_show = document.clone();
        let page_show = Closure::<dyn FnMut(Event)>::new(move |_| {
            if let Some(state) = read_visibility(&document_for_show) {
                show_lifecycle.on_visibility_str(&state);
            }
        });
        if window
            .add_event_listener_with_callback("pageshow", page_show.as_ref().unchecked_ref())
            .is_err()
        {
            let _ = document.remove_event_listener_with_callback(
                "visibilitychange",
                visibility.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "pagehide",
                page_hide.as_ref().unchecked_ref(),
            );
            return Err(BrowserError::Listener);
        }

        Ok(Self { lifecycle, document, window, visibility, page_hide, page_show })
    }

    /// The shared Rust lifecycle registry.
    #[must_use]
    pub fn registry(&self) -> &essenty_lifecycle::LifecycleRegistry {
        self.lifecycle.registry()
    }
}

impl Drop for BrowserLifecycle {
    fn drop(&mut self) {
        let _ = self.document.remove_event_listener_with_callback(
            "visibilitychange",
            self.visibility.as_ref().unchecked_ref(),
        );
        let _ = self.window.remove_event_listener_with_callback(
            "pagehide",
            self.page_hide.as_ref().unchecked_ref(),
        );
        let _ = self.window.remove_event_listener_with_callback(
            "pageshow",
            self.page_show.as_ref().unchecked_ref(),
        );
    }
}

/// Reads `document.visibilityState` from the live DOM.
/// Returns `None` when no window/document is available.
///
/// Reads the raw JS property via `Reflect` rather than the typed
/// `web-sys` enum so the returned string matches
/// [`crate::PageVisibility::parse`] inputs exactly.
#[must_use]
pub fn current_visibility_state() -> Option<String> {
    read_visibility(&window()?.document()?)
}

/// Returns `true` when running inside a browser window.
#[must_use]
pub fn has_window() -> bool {
    window().is_some()
}

/// Casts a generic `Event` to `PopStateEvent` when possible.
#[must_use]
pub fn as_pop_state(event: &web_sys::Event) -> Option<web_sys::PopStateEvent> {
    event.dyn_ref::<web_sys::PopStateEvent>().cloned()
}
