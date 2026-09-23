use super::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Counter {
    value: u32,
}

fn json_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

fn json_decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

#[test]
fn empty_save_yields_empty_map() {
    let keeper = StateKeeper::new();
    assert!(keeper.save().unwrap().is_empty());
    assert_eq!(keeper.provider_count(), 0);
    assert_eq!(keeper.pending_count(), 0);
}

#[test]
fn bytes_round_trip() {
    let mut keeper = StateKeeper::new();
    keeper.register("counter", || 41_u32.to_le_bytes().to_vec()).unwrap();
    let saved = keeper.save().unwrap();

    let mut restored = StateKeeper::with_restored(saved);
    let bytes = restored.consume_bytes("counter").unwrap();
    assert_eq!(u32::from_le_bytes(bytes.try_into().unwrap()), 41);
}

#[test]
fn serde_value_round_trip_with_pluggable_codec() {
    let mut keeper = StateKeeper::new();
    keeper.register_value("counter", || Counter { value: 7 }, json_encode).unwrap();
    let saved = keeper.save().unwrap();

    let mut restored = StateKeeper::with_restored(saved);
    let value: Option<Counter> = restored.consume_value("counter", json_decode).unwrap();
    assert_eq!(value, Some(Counter { value: 7 }));
}

#[test]
fn multiple_keys_save_in_order() {
    let mut keeper = StateKeeper::new();
    keeper.register("b", || vec![2]).unwrap();
    keeper.register("a", || vec![1]).unwrap();
    keeper.register("c", || vec![3]).unwrap();
    let saved = keeper.save().unwrap();
    let keys: Vec<&str> = saved.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["a", "b", "c"]);
}

#[test]
fn consume_is_single_shot() {
    let mut restored =
        StateKeeper::with_restored(BTreeMap::from([("k".to_owned(), vec![1, 2, 3])]));
    assert!(restored.has_consumable("k"));
    assert_eq!(restored.consume_bytes("k"), Some(vec![1, 2, 3]));
    assert!(!restored.has_consumable("k"));
    assert_eq!(restored.consume_bytes("k"), None);

    let mut restored = StateKeeper::with_restored(BTreeMap::from([(
        "v".to_owned(),
        serde_json::to_vec(&Counter { value: 1 }).unwrap(),
    )]));
    let first: Option<Counter> = restored.consume_value("v", json_decode).unwrap();
    assert_eq!(first, Some(Counter { value: 1 }));
    let second: Option<Counter> = restored.consume_value("v", json_decode).unwrap();
    assert_eq!(second, None);
}

#[test]
fn duplicate_registration_is_rejected() {
    let mut keeper = StateKeeper::new();
    keeper.register("k", || vec![1]).unwrap();
    assert_eq!(
        keeper.register("k", || vec![2]).unwrap_err(),
        StateKeeperError::DuplicateKey("k".to_owned())
    );
    assert_eq!(
        keeper.register_value("k", || 1_u8, json_encode).unwrap_err(),
        StateKeeperError::DuplicateKey("k".to_owned())
    );
    // Original provider is intact.
    assert_eq!(keeper.save().unwrap()["k"], vec![1]);
}

#[test]
fn unregister_removes_provider_but_keeps_restored() {
    let mut keeper = StateKeeper::with_restored(BTreeMap::from([("k".to_owned(), vec![9])]));
    keeper.register("k", || vec![1]).unwrap();
    assert!(keeper.has_provider("k"));
    assert!(keeper.unregister("k"));
    assert!(!keeper.has_provider("k"));
    assert!(!keeper.unregister("k"));
    assert_eq!(keeper.save().unwrap().get("k"), Some(&vec![9]));
    // Restored bytes survive unregistration.
    assert_eq!(keeper.consume_bytes("k"), Some(vec![9]));
}

#[test]
fn decode_failure_returns_typed_error() {
    let mut restored = StateKeeper::with_restored(BTreeMap::from([("v".to_owned(), vec![0xFF])]));
    let err = restored.consume_value::<Counter>("v", json_decode).unwrap_err();
    assert!(matches!(err, StateKeeperError::Decode { .. }));
}

#[test]
fn encode_failure_surfaces_at_save() {
    let mut keeper = StateKeeper::new();
    keeper.register_value("bad", || Counter { value: 1 }, |_| Err("boom".to_owned())).unwrap();
    let err = keeper.save().unwrap_err();
    assert_eq!(err, StateKeeperError::Encode { key: "bad".to_owned(), reason: "boom".to_owned() });
}

#[test]
fn unconsumed_restored_values_survive_another_save() {
    let mut keeper = StateKeeper::with_restored(BTreeMap::from([
        ("old".to_owned(), vec![1]),
        ("replaced".to_owned(), vec![2]),
        ("consumed".to_owned(), vec![3]),
    ]));
    assert_eq!(keeper.consume_bytes("consumed"), Some(vec![3]));
    keeper.register("replaced", || vec![9]).unwrap();
    let saved = keeper.save().unwrap();
    assert_eq!(saved.get("old"), Some(&vec![1]));
    assert_eq!(saved.get("replaced"), Some(&vec![9]));
    assert!(!saved.contains_key("consumed"));
}
