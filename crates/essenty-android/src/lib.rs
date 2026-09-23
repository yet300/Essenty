//! Android adapter for the Essenty Rust runtime.
//!
//! Host-compilable mapping adapters coexist with optional native-activity
//! integration. `AndroidX` owner attachment still requires additional host glue.
//!
//! `AndroidX` behavior still to integrate:
//!
//! - `Activity` lifecycle callbacks → [`AndroidLifecycle`]
//! - `SavedStateRegistry` save/restore → [`AndroidStateHost`]
//! - `ViewModelStore` retention → core `InstanceKeeper` (already usable)
//! - `OnBackPressedDispatcher` + Android Predictive Back → [`AndroidBackBridge`]
//!
//! All Android/JNI dependencies must live behind
//! `target.'cfg(target_os = "android")'.dependencies` in this crate and must
//! never leak into the core crates.
//!
//! On Android, the optional `native-activity` feature provides
//! `NativeActivityLifecycle` and `NativeActivityState`, which drive lifecycle
//! and saved-state restoration through the `android-activity` event loop. No
//! application-written Java is needed for that host model.
//!
//! # Example
//!
//! ```rust
//! use essenty_android::AndroidLifecycle;
//!
//! let host = AndroidLifecycle::new();
//! host.on_create();
//! host.on_start();
//! host.on_resume();
//! assert!(host.registry().state().is_resumed());
//! ```

mod back_handler;
mod lifecycle;
mod native_state;
mod state_keeper;

pub use back_handler::AndroidBackBridge;
pub use lifecycle::AndroidLifecycle;
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use native_state::NativeActivityState;
pub use native_state::{NativeStateError, decode_native_state, encode_native_state};
pub use state_keeper::AndroidStateHost;

#[cfg(all(target_os = "android", feature = "native-activity"))]
mod native_activity;
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use native_activity::NativeActivityLifecycle;

#[cfg(test)]
mod tests;
