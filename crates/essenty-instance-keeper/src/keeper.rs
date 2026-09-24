use crate::InstanceKeeperError;
use std::any::Any;
use std::collections::BTreeMap;
use std::rc::Rc;

/// `BTreeMap` gives deterministic key order for lookup and cleanup.
#[derive(Default)]
pub struct InstanceKeeper {
    instances: BTreeMap<String, Rc<dyn Any>>,
    destroyed: bool,
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

    /// Whether the retained scope has ended.
    #[must_use]
    pub fn is_destroyed(&self) -> bool {
        self.destroyed
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
        if self.destroyed {
            return Err(InstanceKeeperError::Destroyed);
        }
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

    /// Stores a prebuilt instance under `key`, returning shared ownership.
    ///
    /// This mirrors upstream `InstanceKeeper.put`: retention of an
    /// already-constructed value (as opposed to [`InstanceKeeper::get_or_create`]
    /// factory retention). Cleanup still follows Rust ownership — the value
    /// drops once the keeper releases it and all external [`Rc`] clones are
    /// gone.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceKeeperError::Destroyed`] when the scope has ended.
    /// Returns [`InstanceKeeperError::DuplicateKey`] when `key` already
    /// holds a value; the retained value is left intact.
    pub fn put<T>(&mut self, key: &str, instance: T) -> Result<Rc<T>, InstanceKeeperError>
    where
        T: 'static,
    {
        if self.destroyed {
            return Err(InstanceKeeperError::Destroyed);
        }
        if self.instances.contains_key(key) {
            return Err(InstanceKeeperError::DuplicateKey(key.to_owned()));
        }
        let retained: Rc<T> = Rc::new(instance);
        self.instances.insert(key.to_owned(), Rc::clone(&retained) as Rc<dyn Any>);
        Ok(retained)
    }

    /// Releases the instance under `key` without destroying it, returning
    /// keeper-owned shared ownership when present.
    ///
    /// This mirrors upstream `InstanceKeeper.remove`, which likewise does not
    /// destroy the instance: the value stays alive while external [`Rc`]
    /// clones exist.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceKeeperError::TypeMismatch`] when `key` holds a
    /// value of a different type; the retained value is left intact.
    pub fn remove<T>(&mut self, key: &str) -> Result<Option<Rc<T>>, InstanceKeeperError>
    where
        T: 'static,
    {
        let Some(existing) = self.instances.remove(key) else {
            return Ok(None);
        };
        Rc::downcast::<T>(existing).map(Some).map_err(|retained| {
            self.instances.insert(key.to_owned(), retained);
            InstanceKeeperError::TypeMismatch(key.to_owned())
        })
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

    /// Ends this scope and releases all keeper-owned references. Subsequent
    /// creation returns [`InstanceKeeperError::Destroyed`]. External `Rc`
    /// clones retain their values until their final owner drops them.
    pub fn destroy_all(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            self.clear();
        }
    }
}
