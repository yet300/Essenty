use super::*;
use std::collections::BTreeMap;

#[test]
fn single_dispatch_routes_lifecycle_back_and_state() {
    let mut runtime = Runtime::new();
    runtime.state_keeper_mut().register("counter", || vec![7]).unwrap();
    runtime.back_dispatcher_mut().register(0, true, |_| {});
    runtime.dispatch(PlatformEvent::Lifecycle(LifecycleState::Resumed)).unwrap();
    assert_eq!(runtime.lifecycle().state(), LifecycleState::Resumed);
    let back = runtime.dispatch(PlatformEvent::BackPressed).unwrap();
    assert_eq!(back.back_handled, Some(true));
    assert!(back.can_handle_back);
    let saved = runtime.dispatch(PlatformEvent::SaveState).unwrap();
    assert_eq!(saved.saved_state.unwrap().get("counter"), Some(&vec![7]));
}

#[test]
fn restored_state_is_consumed_inside_runtime() {
    let mut runtime = Runtime::with_restored(BTreeMap::from([("key".into(), vec![4])]));
    assert_eq!(runtime.state_keeper_mut().consume_bytes("key"), Some(vec![4]));
    assert!(runtime.dispatch(PlatformEvent::SaveState).unwrap().saved_state.unwrap().is_empty());
}

#[test]
fn terminal_lifecycle_event_ends_retained_scope() {
    let mut runtime = Runtime::new();
    runtime.instance_keeper_mut().get_or_create("model", || 1_u32).unwrap();
    runtime.dispatch(PlatformEvent::Lifecycle(LifecycleState::Destroyed)).unwrap();
    assert!(runtime.instance_keeper_mut().is_destroyed());
    assert_eq!(
        runtime.instance_keeper_mut().get_or_create("new", || 2_u32).unwrap_err(),
        InstanceKeeperError::Destroyed
    );
}
