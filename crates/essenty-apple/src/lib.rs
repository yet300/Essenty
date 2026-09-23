//! Apple-family adapter for the Essenty Rust runtime.
//!
//! One crate covers the whole Apple family — iOS, macOS, watchOS, tvOS,
//! visionOS, and Mac Catalyst — with maximally shared implementation. Only
//! genuinely OS-specific behavior gets its own module, gated with
//! `cfg(target_vendor = "apple")` plus OS checks where required.
//!
//! The macOS [`ApplicationLifecycle`] observes `NSApplication` notifications
//! directly through `objc2`. Other Apple targets currently have manual
//! mapping only.
//!
//! # Example
//!
//! ```rust
//! use essenty_apple::AppleLifecycle;
//!
//! let host = AppleLifecycle::new();
//! host.scene_connected();
//! host.scene_became_active();
//! assert!(host.registry().state().is_resumed());
//! ```

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

/// Shared Apple-family helpers (state restoration, gestures).
///
/// Re-exported per-OS modules below delegate here so behavior stays unified.
pub mod shared {
    use super::AppleLifecycle;

    /// Runs the standard foreground activation sequence.
    pub fn activate(host: &AppleLifecycle) {
        host.scene_connected();
        host.scene_will_enter_foreground();
        host.scene_became_active();
    }

    /// Runs the standard background + disconnect sequence.
    pub fn terminate(host: &AppleLifecycle) {
        host.scene_will_resign_active();
        host.scene_did_enter_background();
        host.scene_disconnected();
    }
}

#[cfg(target_os = "macos")]
mod macos_native;

#[cfg(target_os = "macos")]
pub use macos_native::{ApplicationLifecycle, ApplicationLifecycleError};

#[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
mod uikit_native;

#[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
pub use uikit_native::{ApplicationLifecycle, ApplicationLifecycleError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{activate, terminate};

    #[test]
    fn activation_sequence_reaches_resumed() {
        let host = AppleLifecycle::new();
        activate(&host);
        assert!(host.registry().state().is_resumed());
    }

    #[test]
    fn terminate_sequence_destroys() {
        let host = AppleLifecycle::new();
        activate(&host);
        terminate(&host);
        assert!(host.registry().state().is_destroyed());
    }

    #[test]
    fn transitions_are_idempotent() {
        let host = AppleLifecycle::new();
        host.scene_connected();
        host.scene_connected();
        host.scene_became_active();
        assert!(host.registry().state().is_resumed());
        host.scene_will_resign_active();
        assert_eq!(host.registry().state(), LifecycleState::Started);
    }
}
