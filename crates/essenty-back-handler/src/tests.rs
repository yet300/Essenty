use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn no_handler_returns_false() {
    let mut dispatcher = BackDispatcher::new();
    assert!(!dispatcher.can_handle());
    assert!(!dispatcher.back());
    assert!(!dispatcher.predictive_start());
    assert_eq!(dispatcher.predictive_progress(0.5).unwrap_err(), BackError::NoGestureInProgress);
    assert_eq!(dispatcher.predictive_cancel().unwrap_err(), BackError::NoGestureInProgress);
    assert_eq!(dispatcher.predictive_invoke().unwrap_err(), BackError::NoGestureInProgress);
}

#[test]
fn single_handler_receives_back() {
    let mut dispatcher = BackDispatcher::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&log);
    let handle = dispatcher.register(0, true, move |event| probe.borrow_mut().push(event));
    assert!(dispatcher.can_handle());
    assert!(dispatcher.back());
    assert_eq!(*log.borrow(), vec![BackEvent::invoked()]);
    assert!(dispatcher.unregister(handle.id()));
    assert!(!dispatcher.back());
}

#[test]
fn disabled_handler_is_skipped() {
    let mut dispatcher = BackDispatcher::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&log);
    let handle = dispatcher.register(10, false, move |event| {
        probe.borrow_mut().push(event);
    });
    let id = handle.id();
    assert!(!dispatcher.back());
    assert!(log.borrow().is_empty());
    assert!(dispatcher.set_enabled(id, true));
    assert!(dispatcher.is_enabled(id));
    assert!(dispatcher.back());
    assert_eq!(log.borrow().len(), 1);
    dispatcher.set_enabled(id, false);
    assert!(!dispatcher.back());
}

#[test]
fn highest_priority_wins() {
    let mut dispatcher = BackDispatcher::new();
    let low = Rc::new(RefCell::new(0_u32));
    let high = Rc::new(RefCell::new(0_u32));
    let low_probe = Rc::clone(&low);
    let high_probe = Rc::clone(&high);
    dispatcher.register(1, true, move |_| *low_probe.borrow_mut() += 1);
    dispatcher.register(5, true, move |_| *high_probe.borrow_mut() += 1);
    assert!(dispatcher.back());
    assert_eq!(*low.borrow(), 0);
    assert_eq!(*high.borrow(), 1);
}

#[test]
fn later_registration_breaks_priority_ties() {
    let mut dispatcher = BackDispatcher::new();
    let first = Rc::new(RefCell::new(0_u32));
    let second = Rc::new(RefCell::new(0_u32));
    let a = Rc::clone(&first);
    let b = Rc::clone(&second);
    dispatcher.register(0, true, move |_| *a.borrow_mut() += 1);
    dispatcher.register(0, true, move |_| *b.borrow_mut() += 1);
    assert!(dispatcher.back());
    assert_eq!(*first.borrow(), 0);
    assert_eq!(*second.borrow(), 1);
}

#[test]
fn unregister_by_id_stops_delivery() {
    let mut dispatcher = BackDispatcher::new();
    let log = Rc::new(RefCell::new(0_u32));
    let probe = Rc::clone(&log);
    let handle = dispatcher.register(0, true, move |_| *probe.borrow_mut() += 1);
    let id = handle.id();
    assert!(dispatcher.unregister(id));
    assert!(!dispatcher.unregister(id));
    assert!(!dispatcher.back());
    assert_eq!(*log.borrow(), 0);
}

#[test]
fn predictive_gesture_lifecycle() {
    let mut dispatcher = BackDispatcher::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&log);
    let _handle = dispatcher.register(0, true, move |event| {
        probe.borrow_mut().push(event);
    });
    assert!(dispatcher.predictive_start());
    assert!(dispatcher.has_active_gesture());
    assert!(dispatcher.predictive_progress(0.25).unwrap());
    assert!(dispatcher.predictive_progress(0.75).unwrap());
    assert!(dispatcher.predictive_cancel().unwrap());
    assert!(!dispatcher.has_active_gesture());
    assert_eq!(
        *log.borrow(),
        vec![
            BackEvent::started(),
            BackEvent::progressed(0.25),
            BackEvent::progressed(0.75),
            BackEvent::cancelled(),
        ]
    );
}

#[test]
fn predictive_gesture_preserves_edge_and_touch_coordinates() {
    let mut dispatcher = BackDispatcher::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&events);
    dispatcher.register(0, true, move |event| probe.borrow_mut().push(event));

    let start = GesturePosition { swipe_edge: SwipeEdge::Left, touch_x: 12.5, touch_y: 24.0 };
    let progress = GesturePosition { swipe_edge: SwipeEdge::Right, touch_x: 28.0, touch_y: 40.5 };
    assert!(dispatcher.predictive_start_with(start));
    assert!(dispatcher.predictive_progress_with(0.6, progress).unwrap());
    assert!(dispatcher.predictive_cancel().unwrap());

    let events = events.borrow();
    assert_eq!(events[0].position, Some(start));
    assert_eq!(events[1].position, Some(progress));
    assert_eq!(events[1].progress, Some(0.6));
    assert_eq!(events[2].position, None);
}

#[test]
fn predictive_invoke_completes_gesture() {
    let mut dispatcher = BackDispatcher::new();
    let log = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&log);
    let _handle = dispatcher.register(0, true, move |event| {
        probe.borrow_mut().push(event);
    });
    assert!(dispatcher.predictive_start());
    assert!(dispatcher.predictive_invoke().unwrap());
    assert!(!dispatcher.has_active_gesture());
    assert_eq!(*log.borrow(), vec![BackEvent::started(), BackEvent::invoked(),]);
    // Gesture is over; further gesture events error.
    assert_eq!(dispatcher.predictive_progress(0.5).unwrap_err(), BackError::NoGestureInProgress);
}

#[test]
fn gesture_sticks_to_claimed_handler() {
    let mut dispatcher = BackDispatcher::new();
    let first = Rc::new(RefCell::new(Vec::new()));
    let a = Rc::clone(&first);
    dispatcher.register(1, true, move |event| a.borrow_mut().push(event));
    assert!(dispatcher.predictive_start());
    // A new higher-priority handler appears mid-gesture, but the
    // in-flight gesture still routes to the claimed owner.
    let newcomer_calls = Rc::new(RefCell::new(0_u32));
    let newcomer_probe = Rc::clone(&newcomer_calls);
    let _new_high = dispatcher.register(99, true, move |_| {
        *newcomer_probe.borrow_mut() += 1;
    });
    assert!(dispatcher.predictive_progress(0.5).unwrap());
    assert!(dispatcher.predictive_invoke().unwrap());
    assert_eq!(first.borrow().len(), 3);
    assert_eq!(*newcomer_calls.borrow(), 0);
    // After the gesture, the newcomer wins regular back handling.
    assert!(dispatcher.back());
    assert_eq!(*newcomer_calls.borrow(), 1);
}

#[test]
fn callback_can_unregister_itself_and_register_successor() {
    use std::cell::Cell;
    let mut dispatcher = BackDispatcher::new();
    let own_id = Rc::new(Cell::new(u64::MAX));
    let own_id_in_callback = Rc::clone(&own_id);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = Rc::clone(&calls);
    let successor_calls = Rc::clone(&calls);
    let handle = dispatcher.register_reentrant(0, true, move |event, commands| {
        if event.phase == BackPhase::Invoked {
            first_calls.borrow_mut().push("first");
            commands.unregister(own_id_in_callback.get());
            commands.register(0, true, {
                let successor_calls = Rc::clone(&successor_calls);
                move |_, _| successor_calls.borrow_mut().push("successor")
            });
        }
    });
    own_id.set(handle.id());
    assert!(dispatcher.back());
    assert_eq!(dispatcher.handler_count(), 1);
    assert!(dispatcher.back());
    assert_eq!(*calls.borrow(), vec!["first", "successor"]);
}

#[test]
fn removed_gesture_owner_is_cancelled_and_fallback_starts_on_progress() {
    let mut dispatcher = BackDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let older = Rc::clone(&calls);
    let selected = Rc::clone(&calls);
    dispatcher.register(0, true, move |event| older.borrow_mut().push(("older", event.phase)));
    let selected_handle = dispatcher.register(0, true, move |event| {
        selected.borrow_mut().push(("selected", event.phase));
    });
    assert!(dispatcher.predictive_start());
    assert!(dispatcher.unregister(selected_handle.id()));
    assert!(dispatcher.has_active_gesture());
    assert!(dispatcher.predictive_progress(0.4).unwrap());
    assert!(dispatcher.predictive_invoke().unwrap());
    assert_eq!(
        *calls.borrow(),
        vec![
            ("selected", BackPhase::Started),
            ("selected", BackPhase::Cancelled),
            ("older", BackPhase::Started),
            ("older", BackPhase::Progressed),
            ("older", BackPhase::Invoked),
        ]
    );
}

#[test]
fn disabling_gesture_owner_does_not_interrupt_claim() {
    let mut dispatcher = BackDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&calls);
    let handle = dispatcher.register(0, true, move |event| probe.borrow_mut().push(event.phase));
    assert!(dispatcher.predictive_start());
    assert!(dispatcher.set_enabled(handle.id(), false));
    assert!(dispatcher.predictive_progress(0.4).unwrap());
    assert!(dispatcher.predictive_cancel().unwrap());
    assert_eq!(
        *calls.borrow(),
        vec![BackPhase::Started, BackPhase::Progressed, BackPhase::Cancelled]
    );
}

#[test]
fn enabled_changed_listeners_fire_only_on_aggregate_transitions() {
    let mut dispatcher = BackDispatcher::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let probe = Rc::clone(&events);
    let listener = dispatcher.add_enabled_changed_listener(move |enabled| {
        probe.borrow_mut().push(enabled);
    });
    // Disabled registration does not change the aggregate.
    let disabled = dispatcher.register(0, false, |_| {});
    assert!(events.borrow().is_empty());
    // First enabled handler flips the aggregate to true.
    let first = dispatcher.register(0, true, |_| {});
    assert_eq!(*events.borrow(), vec![true]);
    // Second enabled handler leaves the aggregate unchanged.
    let second = dispatcher.register(0, true, |_| {});
    assert_eq!(*events.borrow(), vec![true]);
    // Disabling one of two enabled handlers leaves the aggregate unchanged.
    assert!(dispatcher.set_enabled(first.id(), false));
    assert_eq!(*events.borrow(), vec![true]);
    // Disabling the last enabled handler flips the aggregate to false.
    assert!(dispatcher.set_enabled(second.id(), false));
    assert_eq!(*events.borrow(), vec![true, false]);
    // Re-enabling flips it back; removing the listener stops delivery.
    assert!(dispatcher.set_enabled(disabled.id(), true));
    assert_eq!(*events.borrow(), vec![true, false, true]);
    assert!(dispatcher.remove_enabled_changed_listener(listener));
    assert!(!dispatcher.remove_enabled_changed_listener(listener));
    assert!(dispatcher.unregister(first.id()));
    assert_eq!(*events.borrow(), vec![true, false, true]);
}

#[test]
fn set_priority_changes_winner_selection() {
    let mut dispatcher = BackDispatcher::new();
    let low = Rc::new(RefCell::new(0_u32));
    let high = Rc::new(RefCell::new(0_u32));
    let low_probe = Rc::clone(&low);
    let high_probe = Rc::clone(&high);
    let low_handle = dispatcher.register(0, true, move |_| *low_probe.borrow_mut() += 1);
    let high_handle = dispatcher.register(0, true, move |_| *high_probe.borrow_mut() += 1);
    // Tie breaks toward the later registration.
    assert!(dispatcher.back());
    assert_eq!((*low.borrow(), *high.borrow()), (0, 1));
    assert_eq!(dispatcher.priority(low_handle.id()), Some(0));
    // Raising the earlier handler's priority makes it win.
    assert!(dispatcher.set_priority(low_handle.id(), 3));
    assert_eq!(dispatcher.priority(low_handle.id()), Some(3));
    assert!(dispatcher.back());
    assert_eq!((*low.borrow(), *high.borrow()), (1, 1));
    assert!(!dispatcher.set_priority(999, 1));
    assert_eq!(dispatcher.priority(high_handle.id()), Some(0));
    assert_eq!(dispatcher.priority(999), None);
}

#[test]
fn fallback_after_owner_removal_replays_original_start_event() {
    let mut dispatcher = BackDispatcher::new();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let older = Rc::clone(&calls);
    let selected = Rc::clone(&calls);
    dispatcher.register(0, true, move |event| older.borrow_mut().push(("older", event)));
    let selected_handle = dispatcher.register(0, true, move |event| {
        selected.borrow_mut().push(("selected", event));
    });
    let start_position =
        GesturePosition { swipe_edge: SwipeEdge::Left, touch_x: 12.5, touch_y: 24.0 };
    assert!(dispatcher.predictive_start_with(start_position));
    assert!(dispatcher.unregister(selected_handle.id()));
    let progress_position =
        GesturePosition { swipe_edge: SwipeEdge::Right, touch_x: 28.0, touch_y: 40.5 };
    assert!(dispatcher.predictive_progress_with(0.4, progress_position).unwrap());
    let log = calls.borrow();
    assert_eq!(log.len(), 4);
    // Selected owner saw the positioned start, then cancellation.
    assert_eq!(log[0].1, BackEvent::started_with(start_position));
    assert_eq!(log[0].1.phase, BackPhase::Started);
    assert_eq!(log[1].1.phase, BackPhase::Cancelled);
    // Fallback first receives the *original* start event, then progress.
    assert_eq!(log[2].1, BackEvent::started_with(start_position));
    assert_eq!(log[3].1, BackEvent::progressed_with(0.4, progress_position));
}
