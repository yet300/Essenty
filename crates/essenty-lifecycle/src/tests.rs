use super::*;
use std::cell::RefCell as StdRefCell;
use std::rc::Rc as StdRc;

#[test]
fn initial_state_is_initialized() {
    let lifecycle = LifecycleRegistry::new();
    assert_eq!(lifecycle.state(), LifecycleState::Initialized);
    assert_eq!(lifecycle.subscriber_count(), 0);
}

#[test]
fn full_forward_and_backward_cycle() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Created);
    lifecycle.start().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Started);
    lifecycle.resume().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Resumed);
    lifecycle.pause().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Started);
    lifecycle.stop().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Created);
    lifecycle.destroy().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Destroyed);
}

#[test]
fn invalid_strict_steps_are_rejected() {
    let lifecycle = LifecycleRegistry::new();
    assert_eq!(
        lifecycle.start().unwrap_err(),
        LifecycleError::InvalidTransition {
            from: LifecycleState::Initialized,
            to: LifecycleState::Started,
        }
    );
    assert_eq!(
        lifecycle.resume().unwrap_err(),
        LifecycleError::InvalidTransition {
            from: LifecycleState::Initialized,
            to: LifecycleState::Resumed,
        }
    );
    assert_eq!(
        lifecycle.pause().unwrap_err(),
        LifecycleError::InvalidTransition {
            from: LifecycleState::Initialized,
            to: LifecycleState::Started,
        }
    );
    lifecycle.create().unwrap();
    assert!(lifecycle.resume().is_err());
    assert!(lifecycle.pause().is_err());
}

#[test]
fn destroyed_is_terminal() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.destroy().unwrap();
    assert_eq!(lifecycle.destroy().unwrap_err(), LifecycleError::AlreadyDestroyed);
    assert_eq!(
        lifecycle.move_to(LifecycleState::Created).unwrap_err(),
        LifecycleError::AlreadyDestroyed
    );
    assert_eq!(lifecycle.create().unwrap_err(), LifecycleError::AlreadyDestroyed);
}

#[test]
fn move_to_walks_intermediate_states_in_order() {
    let lifecycle = LifecycleRegistry::new();
    let seen = StdRc::new(StdRefCell::new(Vec::new()));
    let probe = StdRc::clone(&seen);
    let _guard = lifecycle.subscribe(move |state| probe.borrow_mut().push(state));
    lifecycle.move_to(LifecycleState::Resumed).unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![LifecycleState::Created, LifecycleState::Started, LifecycleState::Resumed,]
    );
    lifecycle.move_to(LifecycleState::Created).unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![
            LifecycleState::Created,
            LifecycleState::Started,
            LifecycleState::Resumed,
            LifecycleState::Started,
            LifecycleState::Created,
        ]
    );
}

#[test]
fn move_to_current_state_is_noop() {
    let lifecycle = LifecycleRegistry::new();
    let calls = StdRc::new(StdRefCell::new(0_u32));
    let probe = StdRc::clone(&calls);
    let _guard = lifecycle.subscribe(move |_| *probe.borrow_mut() += 1);
    lifecycle.move_to(LifecycleState::Initialized).unwrap();
    assert_eq!(*calls.borrow(), 0);
}

#[test]
fn observers_fire_in_subscription_order() {
    let lifecycle = LifecycleRegistry::new();
    let log = StdRc::new(StdRefCell::new(Vec::new()));
    let a = StdRc::clone(&log);
    let b = StdRc::clone(&log);
    let _ga = lifecycle.subscribe(move |_| a.borrow_mut().push("a"));
    let _gb = lifecycle.subscribe(move |_| b.borrow_mut().push("b"));
    lifecycle.create().unwrap();
    assert_eq!(*log.borrow(), vec!["a", "b"]);
}

#[test]
fn dropping_subscription_unsubscribes() {
    let lifecycle = LifecycleRegistry::new();
    let calls = StdRc::new(StdRefCell::new(0_u32));
    let probe = StdRc::clone(&calls);
    let guard = lifecycle.subscribe(move |_| *probe.borrow_mut() += 1);
    assert_eq!(lifecycle.subscriber_count(), 1);
    assert!(guard.is_active());
    drop(guard);
    assert_eq!(lifecycle.subscriber_count(), 0);
    lifecycle.create().unwrap();
    assert_eq!(*calls.borrow(), 0);
}

#[test]
fn registry_clone_shares_state() {
    let lifecycle = LifecycleRegistry::new();
    let other = lifecycle.clone();
    lifecycle.create().unwrap();
    assert_eq!(other.state(), LifecycleState::Created);
}

#[test]
fn reentrant_subscribe_from_callback_is_safe() {
    let lifecycle = LifecycleRegistry::new();
    let lifecycle_ref = lifecycle.clone();
    let fired = StdRc::new(StdRefCell::new(Vec::new()));
    let probe = StdRc::clone(&fired);
    let _guard = lifecycle.subscribe(move |state| {
        probe.borrow_mut().push(state);
        if state == LifecycleState::Created {
            let probe2 = StdRc::clone(&probe);
            // Detach so the new observer outlives this callback frame.
            let _ = lifecycle_ref.subscribe(move |s| probe2.borrow_mut().push(s)).detach();
        }
    });
    lifecycle.create().unwrap();
    lifecycle.start().unwrap();
    // The new observer first receives a Created replay, then Started.
    assert_eq!(fired.borrow().len(), 4);
}

#[test]
fn destroy_notifies_observers_once() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Resumed).unwrap();
    let seen = StdRc::new(StdRefCell::new(Vec::new()));
    let probe = StdRc::clone(&seen);
    let _guard = lifecycle.subscribe(move |state| probe.borrow_mut().push(state));
    lifecycle.destroy().unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![
            LifecycleState::Created,
            LifecycleState::Started,
            LifecycleState::Resumed,
            LifecycleState::Started,
            LifecycleState::Created,
            LifecycleState::Destroyed,
        ]
    );
}

#[test]
fn reverse_events_notify_in_reverse_subscription_order() {
    let lifecycle = LifecycleRegistry::new();
    let log = StdRc::new(StdRefCell::new(Vec::new()));
    let a = StdRc::clone(&log);
    let b = StdRc::clone(&log);
    let _a = lifecycle.subscribe(move |state| a.borrow_mut().push(("a", state)));
    let _b = lifecycle.subscribe(move |state| b.borrow_mut().push(("b", state)));
    lifecycle.move_to(LifecycleState::Resumed).unwrap();
    log.borrow_mut().clear();
    lifecycle.destroy().unwrap();
    assert_eq!(
        *log.borrow(),
        vec![
            ("b", LifecycleState::Started),
            ("a", LifecycleState::Started),
            ("b", LifecycleState::Created),
            ("a", LifecycleState::Created),
            ("b", LifecycleState::Destroyed),
            ("a", LifecycleState::Destroyed),
        ]
    );
    assert_eq!(lifecycle.subscriber_count(), 0);
}

#[test]
fn late_subscriber_replays_current_state_then_receives_future_events() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Started).unwrap();
    let seen = StdRc::new(StdRefCell::new(Vec::new()));
    let probe = StdRc::clone(&seen);
    let _guard = lifecycle.subscribe(move |state| probe.borrow_mut().push(state));
    lifecycle.resume().unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![LifecycleState::Created, LifecycleState::Started, LifecycleState::Resumed]
    );
}

#[test]
fn observer_can_unsubscribe_itself() {
    use std::cell::Cell;
    let lifecycle = LifecycleRegistry::new();
    let id = StdRc::new(Cell::new(u64::MAX));
    let id_in_callback = StdRc::clone(&id);
    let nested = lifecycle.clone();
    let calls = StdRc::new(Cell::new(0_u32));
    let calls_in_callback = StdRc::clone(&calls);
    let handle = lifecycle.subscribe(move |_| {
        calls_in_callback.set(calls_in_callback.get() + 1);
        let _ = nested.unsubscribe(id_in_callback.get());
    });
    id.set(handle.detach());
    lifecycle.move_to(LifecycleState::Resumed).unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn removed_observer_still_receives_current_snapshot_event() {
    use std::cell::Cell;
    let lifecycle = LifecycleRegistry::new();
    let target_id = StdRc::new(Cell::new(u64::MAX));
    let target_in_callback = StdRc::clone(&target_id);
    let nested = lifecycle.clone();
    let _first = lifecycle.subscribe(move |_| {
        let _ = nested.unsubscribe(target_in_callback.get());
    });
    let calls = StdRc::new(Cell::new(0_u32));
    let probe = StdRc::clone(&calls);
    target_id.set(lifecycle.subscribe(move |_| probe.set(probe.get() + 1)).detach());
    lifecycle.create().unwrap();
    lifecycle.start().unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn callback_can_destroy_during_create() {
    let lifecycle = LifecycleRegistry::new();
    let nested = lifecycle.clone();
    let _guard = lifecycle.subscribe(move |state| {
        if state == LifecycleState::Created {
            nested.destroy().unwrap();
        }
    });
    lifecycle.create().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Destroyed);
}
