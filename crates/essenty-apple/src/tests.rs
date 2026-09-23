use super::*;
use crate::shared::{activate, terminate};
use essenty_lifecycle::LifecycleState;

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
