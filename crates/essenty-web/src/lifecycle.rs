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
