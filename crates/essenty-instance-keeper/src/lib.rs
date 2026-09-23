//! Pure Rust retained-object keeper.
//!
//! Inspired by Essenty's `InstanceKeeper` / ViewModel-like retention: values
//! are retained under string keys and survive events (such as configuration
//! changes or navigation) that would otherwise recreate their owner. Unlike
//! the Kotlin API, retention here follows Rust ownership: values are shared
//! with [`std::rc::Rc`], and normal [`Drop`] runs when the last owner is released.
//!
//! Semantics:
//!
//! - [`InstanceKeeper::get_or_create`] retains one instance per key; repeated
//!   calls with the same key and type return the same [`std::rc::Rc`].
//! - [`InstanceKeeper::get`] returns a clone of the retained [`std::rc::Rc`] without
//!   creating.
//! - [`InstanceKeeper::destroy`] / [`InstanceKeeper::clear`] release the
//!   keeper's ownership. The value is dropped once all external clones are
//!   also gone, at which point [`Drop`] cleanup runs.
//! - [`InstanceKeeper::destroy_all`] ends the whole retention scope.
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

mod error;
mod keeper;

pub use error::InstanceKeeperError;
pub use keeper::InstanceKeeper;

#[cfg(test)]
mod tests;
