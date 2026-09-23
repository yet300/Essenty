use essenty_lifecycle::{LifecycleRegistry, LifecycleState};

/// Shared Apple lifecycle host.
///
/// Accepts scene/app transitions from any Apple OS and forwards them into a
/// core [`LifecycleRegistry`]. Method names follow `UIScene` conventions
/// (`scene_connected`, `scene_became_active`, ...); `NSApplication` and
/// `SwiftUI` hosts map onto the same calls.
#[derive(Debug, Clone, Default)]
pub struct AppleLifecycle {
    registry: LifecycleRegistry,
}

impl AppleLifecycle {
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

    /// Scene/session connected (or app finished launching).
    pub fn scene_connected(&self) {
        let _ = self.registry.move_to(LifecycleState::Created);
    }

    /// Scene moved to foreground (inactive but visible).
    pub fn scene_will_enter_foreground(&self) {
        let _ = self.registry.move_to(LifecycleState::Started);
    }

    /// Scene became active and interactive.
    pub fn scene_became_active(&self) {
        let _ = self.registry.move_to(LifecycleState::Resumed);
    }

    /// Scene will resign active (still visible).
    pub fn scene_will_resign_active(&self) {
        let _ = self.registry.move_to(LifecycleState::Started);
    }

    /// Scene moved to background.
    pub fn scene_did_enter_background(&self) {
        let _ = self.registry.move_to(LifecycleState::Created);
    }

    /// Scene disconnected (or app terminating).
    pub fn scene_disconnected(&self) {
        let _ = self.registry.destroy();
    }
}
