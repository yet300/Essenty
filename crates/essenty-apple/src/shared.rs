/// Shared Apple-family helpers (state restoration, gestures).
///
/// Re-exported per-OS modules below delegate here so behavior stays unified.
use crate::AppleLifecycle;

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
