//! Pure Rust retained-object keeper.
//!
//! Inspired by Essenty's `InstanceKeeper` / ViewModel-like retention: values
//! are retained under string keys and survive events (such as configuration
//! changes or navigation) that would otherwise recreate their owner. Unlike
//! the Kotlin API, retention here follows Rust ownership: values are shared
//! with [`Rc`], and normal [`Drop`] runs when the last owner is released.
//!
//! Semantics:
//!
//! - [`InstanceKeeper::get_or_create`] retains one instance per key; repeated
//!   calls with the same key and type return the same [`Rc`].
//! - [`InstanceKeeper::get`] returns a clone of the retained [`Rc`] without
//!   creating.
//! - [`InstanceKeeper::destroy`] / [`InstanceKeeper::clear`] release the
//!   keeper's ownership. The value is dropped once all external clones are
//!   also gone, at which point [`Drop`] cleanup runs.
//! - Requesting a key with a different type than the retained value returns
//!   [`InstanceKeeperError::TypeMismatch`] instead of panicking.
//!
//! The keeper is single-threaded (`!Send`, `!Sync`) by design so it works on
//! WebAssembly and in single-threaded UI runtimes. Multithreaded hosts can
//! confine the keeper to one thread or wrap it in a `Mutex`.
//!
//! Currently requires `std`; only `alloc`/`core` containers are used, so
//! future `no_std + alloc` support is realistic.
//!
//! # Example
//!
//! ```rust
//! use essenty_instance_keeper::InstanceKeeper;
//!
//! let mut keeper = InstanceKeeper::new();
//! let a = keeper.get_or_create("model", || vec![1, 2, 3]).unwrap();
//! let b = keeper.get_or_create("model", || vec![9, 9, 9]).unwrap();
//! assert!(std::rc::Rc::ptr_eq(&a, &b));
//! ```

use std::any::Any;
use std::collections::BTreeMap;
use std::rc::Rc;

use thiserror::Error;

/// Typed errors returned by [`InstanceKeeper`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstanceKeeperError {
    /// The key is retained but holds a different type than requested.
    #[error("type mismatch for instance key '{0}'")]
    TypeMismatch(String),
}

/// Retained-object keeper keyed by strings.
///
/// `BTreeMap` gives deterministic iteration/destruction order (key order).
#[derive(Default)]
pub struct InstanceKeeper {
    instances: BTreeMap<String, Rc<dyn Any>>,
}

impl std::fmt::Debug for InstanceKeeper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Retained values intentionally omitted: `dyn Any` has no `Debug`.
        f.debug_struct("InstanceKeeper")
            .field("keys", &self.instances.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl InstanceKeeper {
    /// Creates an empty keeper.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of retained instances.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Returns `true` when no instances are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Returns `true` when an instance is retained under `key`.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.instances.contains_key(key)
    }

    /// Returns the retained instance, creating and retaining it when absent.
    ///
    /// Repeated calls with the same key and type return clones of the same
    /// [`Rc`] (the factory runs at most once per key).
    ///
    /// # Errors
    ///
    /// Returns [`InstanceKeeperError::TypeMismatch`] when `key` already
    /// holds a value of a different type; the retained value is left intact
    /// and the factory is not invoked.
    pub fn get_or_create<T, F>(
        &mut self,
        key: &str,
        create: F,
    ) -> Result<Rc<T>, InstanceKeeperError>
    where
        T: 'static,
        F: FnOnce() -> T,
    {
        if let Some(existing) = self.instances.get(key) {
            return Rc::downcast::<T>(Rc::clone(existing))
                .map_err(|_| InstanceKeeperError::TypeMismatch(key.to_owned()));
        }
        let created: Rc<T> = Rc::new(create());
        self.instances.insert(key.to_owned(), Rc::clone(&created) as Rc<dyn Any>);
        Ok(created)
    }

    /// Returns the retained instance without creating it.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceKeeperError::TypeMismatch`] when `key` holds a
    /// value of a different type.
    pub fn get<T>(&self, key: &str) -> Result<Option<Rc<T>>, InstanceKeeperError>
    where
        T: 'static,
    {
        let Some(existing) = self.instances.get(key) else {
            return Ok(None);
        };
        Rc::downcast::<T>(Rc::clone(existing))
            .map(Some)
            .map_err(|_| InstanceKeeperError::TypeMismatch(key.to_owned()))
    }

    /// Releases the instance under `key`. Returns `true` when one existed.
    ///
    /// The value itself is dropped once all external [`Rc`] clones are gone.
    pub fn destroy(&mut self, key: &str) -> bool {
        self.instances.remove(key).is_some()
    }

    /// Releases all retained instances (deterministic key order removal).
    ///
    /// Values with outstanding external clones drop when those clones drop.
    pub fn clear(&mut self) {
        self.instances.clear();
    }
}

#[cfg(test)]
mod tests {
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
}
