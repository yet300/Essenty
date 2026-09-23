use super::*;
use essenty_lifecycle::LifecycleState;
use std::collections::BTreeMap;

#[test]
fn storage_encoding_round_trips_binary_and_rejects_corruption() {
    let bytes = [0, 1, 127, 255];
    let encoded = crate::storage::encode_bytes(&bytes);
    assert_eq!(encoded, "00017fff");
    assert_eq!(crate::storage::decode_bytes(&encoded), Some(bytes.to_vec()));
    assert_eq!(crate::storage::decode_bytes("f"), None);
    assert_eq!(crate::storage::decode_bytes("gg"), None);
}

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
