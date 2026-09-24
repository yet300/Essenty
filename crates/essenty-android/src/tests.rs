use super::*;
use essenty_back_handler::{BackEvent, GesturePosition, SwipeEdge};
use essenty_lifecycle::LifecycleState;

#[test]
fn activity_lifecycle_maps_to_registry() {
    let host = AndroidLifecycle::new();
    host.on_create().unwrap();
    host.on_start().unwrap();
    host.on_resume().unwrap();
    assert!(host.registry().state().is_resumed());
    host.on_pause().unwrap();
    assert_eq!(host.registry().state(), LifecycleState::Started);
    host.on_stop().unwrap();
    assert_eq!(host.registry().state(), LifecycleState::Created);
    host.on_destroy().unwrap();
    assert!(host.registry().state().is_destroyed());
}

#[test]
fn duplicate_callbacks_are_harmless() {
    let host = AndroidLifecycle::new();
    host.on_create().unwrap();
    host.on_create().unwrap();
    host.on_start().unwrap();
    host.on_start().unwrap();
    assert_eq!(host.registry().state(), LifecycleState::Started);
}

#[test]
fn state_host_save_round_trip() {
    let mut host = AndroidStateHost::new();
    host.keeper_mut().register("k", || vec![1, 2]).unwrap();
    let saved = host.perform_save().unwrap();
    let mut restored = AndroidStateHost::with_restored(saved);
    assert_eq!(restored.keeper_mut().consume_bytes("k"), Some(vec![1, 2]));
}

#[test]
fn back_bridge_forwards_press_and_gesture() {
    let mut bridge = AndroidBackBridge::new();
    assert!(!bridge.handle_back_pressed());
    let calls = std::rc::Rc::new(std::cell::RefCell::new(0_u32));
    let probe = std::rc::Rc::clone(&calls);
    bridge.dispatcher_mut().register(0, true, move |_| *probe.borrow_mut() += 1);
    assert!(bridge.handle_back_pressed());
    assert!(bridge.handle_gesture_start());
    assert!(bridge.handle_gesture_progress(0.5).unwrap());
    assert!(bridge.handle_gesture_invoke().unwrap());
    assert_eq!(*calls.borrow(), 4);
}

#[test]
fn back_bridge_preserves_predictive_position() {
    let mut bridge = AndroidBackBridge::new();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::<BackEvent>::new()));
    let probe = std::rc::Rc::clone(&events);
    bridge.dispatcher_mut().register(0, true, move |event| probe.borrow_mut().push(event));

    let position = GesturePosition { swipe_edge: SwipeEdge::Left, touch_x: 8.0, touch_y: 16.0 };
    assert!(bridge.handle_gesture_start_with(position));
    assert!(bridge.handle_gesture_progress_with(0.75, position).unwrap());
    assert!(bridge.handle_gesture_invoke().unwrap());
    assert_eq!(events.borrow()[0].position, Some(position));
    assert_eq!(events.borrow()[1].position, Some(position));
    assert_eq!(events.borrow()[1].progress, Some(0.75));
}
