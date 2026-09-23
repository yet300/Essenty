use super::*;
use std::cell::Cell;
use std::rc::Rc as StdRc;

struct Probe(StdRc<Cell<u32>>);

impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn get_or_create_returns_same_instance() {
    let mut keeper = InstanceKeeper::new();
    let calls = Cell::new(0_u32);
    let a = keeper
        .get_or_create("model", || {
            calls.set(calls.get() + 1);
            vec![1, 2, 3]
        })
        .unwrap();
    let b: StdRc<Vec<i32>> = keeper.get_or_create("model", || vec![9]).unwrap();
    assert_eq!(calls.get(), 1, "factory must run exactly once");
    assert!(StdRc::ptr_eq(&a, &b));
    assert_eq!(*a, vec![1, 2, 3]);
}

#[test]
fn get_returns_none_until_created() {
    let mut keeper = InstanceKeeper::new();
    assert_eq!(keeper.get::<String>("missing").unwrap(), None);
    keeper.get_or_create("s", || "hello".to_owned()).unwrap();
    assert_eq!(keeper.get::<String>("s").unwrap().as_ref().map(|s| s.as_str()), Some("hello"));
}

#[test]
fn multiple_keys_are_independent() {
    let mut keeper = InstanceKeeper::new();
    keeper.get_or_create("a", || 1_u32).unwrap();
    keeper.get_or_create("b", || "x".to_owned()).unwrap();
    assert!(keeper.contains("a"));
    assert!(keeper.contains("b"));
    assert_eq!(keeper.len(), 2);
    assert_eq!(*keeper.get::<u32>("a").unwrap().unwrap(), 1);
}

#[test]
fn type_mismatch_returns_error_and_keeps_value() {
    let mut keeper = InstanceKeeper::new();
    keeper.get_or_create("k", || 1_u32).unwrap();
    assert_eq!(
        keeper.get_or_create("k", || "x".to_owned()).unwrap_err(),
        InstanceKeeperError::TypeMismatch("k".to_owned())
    );
    assert_eq!(
        keeper.get::<String>("k").unwrap_err(),
        InstanceKeeperError::TypeMismatch("k".to_owned())
    );
    // Original value intact.
    assert_eq!(*keeper.get::<u32>("k").unwrap().unwrap(), 1);
}

#[test]
fn destroy_releases_and_allows_recreation() {
    let mut keeper = InstanceKeeper::new();
    let a = keeper.get_or_create("k", || vec![1]).unwrap();
    assert!(keeper.destroy("k"));
    assert!(!keeper.destroy("k"));
    assert!(!keeper.contains("k"));
    // `a` still alive externally; recreating yields a distinct instance.
    let b = keeper.get_or_create("k", || vec![2]).unwrap();
    assert!(!StdRc::ptr_eq(&a, &b));
    assert_eq!(*b, vec![2]);
}

#[test]
fn clear_releases_everything() {
    let mut keeper = InstanceKeeper::new();
    keeper.get_or_create("a", || 1_u32).unwrap();
    keeper.get_or_create("b", || 2_u32).unwrap();
    keeper.clear();
    assert!(keeper.is_empty());
    assert_eq!(keeper.get::<u32>("a").unwrap(), None);
}

#[test]
fn value_drops_when_keeper_and_clones_are_gone() {
    let drops = StdRc::new(Cell::new(0_u32));
    {
        let mut keeper = InstanceKeeper::new();
        let retained = keeper.get_or_create("p", || Probe(StdRc::clone(&drops))).unwrap();
        assert_eq!(drops.get(), 0);
        keeper.clear();
        // External clone keeps the value alive.
        assert_eq!(drops.get(), 0);
        drop(retained);
        assert_eq!(drops.get(), 1);
        // Keeper itself drops here with nothing retained.
    }
    assert_eq!(drops.get(), 1);
}

#[test]
fn keeper_drop_releases_retained_values() {
    let drops = StdRc::new(Cell::new(0_u32));
    {
        let mut keeper = InstanceKeeper::new();
        let _ = keeper.get_or_create("p", || Probe(StdRc::clone(&drops)));
        assert_eq!(drops.get(), 0);
    }
    assert_eq!(drops.get(), 1);
}

#[test]
fn terminal_destroy_is_idempotent_and_rejects_new_instances() {
    let mut keeper = InstanceKeeper::new();
    keeper.get_or_create("old", || 3_u32).unwrap();
    keeper.destroy_all();
    keeper.destroy_all();
    assert!(keeper.is_destroyed());
    assert!(keeper.is_empty());
    assert_eq!(keeper.get_or_create("new", || 4_u32).unwrap_err(), InstanceKeeperError::Destroyed);
}
