//! Web/WASM adapter for the Essenty Rust runtime.
//!
//! Bootstrap scope: host-compilable, pure Rust mapping logic plus narrow
//! `wasm32`-only bindings. Browser APIs (`wasm-bindgen`, `js-sys`,
//! `web-sys`) are used exclusively inside this crate and never leak into the
//! core crates.
//!
//! Integration surfaces:
//!
//! - Page Visibility API → lifecycle ([`VisibilityLifecycle`])
//! - History API / `popstate` → back handling ([`HistoryBackBridge`])
//! - `sessionStorage` / `localStorage` → state persistence ([`StorageKey`])
//!
//! `wasm::BrowserLifecycle` installs live browser lifecycle listeners on
//! `wasm32`. Mapping logic remains host-testable.
//!
//! # Example
//!
//! ```rust
//! use essenty_web::VisibilityLifecycle;
//!
//! let host = VisibilityLifecycle::new();
//! host.on_visibility_str("visible");
//! assert!(host.registry().state().is_resumed());
//! ```

use std::collections::BTreeMap;

use essenty_back_handler::BackDispatcher;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};

/// Page Visibility API states we recognize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageVisibility {
    /// Document is visible.
    Visible,
    /// Document is hidden.
    Hidden,
    /// Pre-render or unrecognized state; treated as hidden.
    Other,
}

impl PageVisibility {
    /// Parses `document.visibilityState` values.
    #[must_use]
    pub fn parse(state: &str) -> Self {
        match state {
            "visible" => Self::Visible,
            "hidden" => Self::Hidden,
            _ => Self::Other,
        }
    }
}

/// Forwards Page Visibility changes into a [`LifecycleRegistry`].
///
/// Mapping: `visible` → `Resumed` (via `Created`/`Started` intermediates),
/// anything else → `Created` (backgrounded but restorable). Non-persisted
/// page hide destroys the registry; back/forward-cache pages remain restorable.
#[derive(Debug, Clone, Default)]
pub struct VisibilityLifecycle {
    registry: LifecycleRegistry,
}

impl VisibilityLifecycle {
    /// Creates a host in `Initialized`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the underlying registry.
    #[must_use]
    pub fn registry(&self) -> &LifecycleRegistry {
        &self.registry
    }

    /// Handles a `visibilitychange` with a parsed [`PageVisibility`].
    pub fn on_visibility(&self, visibility: PageVisibility) {
        match visibility {
            PageVisibility::Visible => {
                let _ = self.registry.move_to(LifecycleState::Resumed);
            }
            PageVisibility::Hidden | PageVisibility::Other => {
                if !self.registry.state().is_destroyed() {
                    let _ = self.registry.move_to(LifecycleState::Created);
                }
            }
        }
    }

    /// Handles a raw `document.visibilityState` string.
    pub fn on_visibility_str(&self, state: &str) {
        self.on_visibility(PageVisibility::parse(state));
    }

    /// Handles page termination (`pagehide` without persistence).
    pub fn on_page_hide(&self) {
        self.on_page_hide_with_persistence(false);
    }

    /// Handles `pagehide`. A persisted page remains eligible for the browser
    /// back/forward cache and can become visible again on `pageshow`.
    pub fn on_page_hide_with_persistence(&self, persisted: bool) {
        if persisted {
            let _ = self.registry.move_to(LifecycleState::Created);
        } else {
            let _ = self.registry.destroy();
        }
    }
}

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

/// Storage area selector for state persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageArea {
    /// `sessionStorage`: per-tab, survives reload.
    Session,
    /// `localStorage`: persists across sessions.
    Local,
}

/// Namespaced storage key helper.
///
/// State maps are flat (`key -> bytes`); hosts namespace them per component
/// (e.g. `"essenty:<component>:<key>"`) and encode bytes off-crate (base64
/// or similar) before writing to Web Storage, keeping this crate free of
/// encoding opinions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageKey {
    area: StorageArea,
    namespace: String,
}

impl StorageKey {
    /// Creates a namespaced key helper.
    #[must_use]
    pub fn new(area: StorageArea, namespace: impl Into<String>) -> Self {
        Self { area, namespace: namespace.into() }
    }

    /// Storage area.
    #[must_use]
    pub fn area(self) -> StorageArea {
        self.area
    }

    /// Fully qualified storage key for a state entry.
    #[must_use]
    pub fn qualified(&self, key: &str) -> String {
        format!("essenty:{}:{key}", self.namespace)
    }

    /// Qualifies every entry of a saved state map.
    #[must_use]
    pub fn qualify_all(&self, saved: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
        saved.iter().map(|(key, value)| (self.qualified(key), value.clone())).collect()
    }
}

/// `wasm32`-only live browser bindings.
///
/// Live browser lifecycle binding and small DOM helpers.
#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use super::VisibilityLifecycle;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_maps_to_lifecycle() {
        let host = VisibilityLifecycle::new();
        host.on_visibility_str("visible");
        assert!(host.registry().state().is_resumed());
        host.on_visibility_str("hidden");
        assert_eq!(host.registry().state(), LifecycleState::Created);
        host.on_visibility(PageVisibility::Other);
        assert_eq!(host.registry().state(), LifecycleState::Created);
        host.on_page_hide();
        assert!(host.registry().state().is_destroyed());
    }

    #[test]
    fn persisted_page_hide_preserves_lifecycle_for_bfcache() {
        let host = VisibilityLifecycle::new();
        host.on_visibility_str("visible");
        host.on_page_hide_with_persistence(true);
        assert_eq!(host.registry().state(), LifecycleState::Created);
        host.on_visibility_str("visible");
        assert_eq!(host.registry().state(), LifecycleState::Resumed);
    }

    #[test]
    fn unknown_visibility_strings_treated_as_hidden() {
        assert_eq!(PageVisibility::parse("prerender"), PageVisibility::Other);
        let host = VisibilityLifecycle::new();
        host.on_visibility_str("prerender");
        assert_eq!(host.registry().state(), LifecycleState::Created);
    }

    #[test]
    fn pop_state_routes_to_dispatcher() {
        let mut bridge = HistoryBackBridge::new();
        assert!(!bridge.on_pop_state());
        let calls = std::rc::Rc::new(std::cell::RefCell::new(0_u32));
        let probe = std::rc::Rc::clone(&calls);
        bridge.dispatcher_mut().register(0, true, move |_| *probe.borrow_mut() += 1);
        assert!(bridge.on_pop_state());
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn storage_keys_are_namespaced() {
        let keys = StorageKey::new(StorageArea::Session, "root");
        assert_eq!(keys.qualified("counter"), "essenty:root:counter");
        let saved = BTreeMap::from([("a".to_owned(), vec![1]), ("b".to_owned(), vec![2])]);
        let qualified = keys.qualify_all(&saved);
        assert!(qualified.contains_key("essenty:root:a"));
        assert!(qualified.contains_key("essenty:root:b"));
        assert_eq!(keys.clone().area(), StorageArea::Session);
    }
}
