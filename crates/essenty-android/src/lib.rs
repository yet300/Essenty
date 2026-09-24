//! Rust-native adapters for Android Essenty semantics.
//!
//! The optional `native-activity` feature integrates Lifecycle, `StateKeeper`,
//! and `BackHandler` with `android-activity` `NativeActivity` and direct platform
//! APIs. `AndroidX` is not part of the primary backend. `InstanceKeeper` retention
//! across Activity recreation is not yet implemented because the core keeper
//! is thread-confined and `NativeActivity` does not provide a proven handoff.
//! Android/JNI dependencies stay inside this crate and never enter core crates.
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
#[cfg(all(target_os = "android", feature = "native-activity"))]
mod back_handler_platform;
mod lifecycle;
mod native_state;
mod state_keeper;

pub use back_handler::AndroidBackBridge;
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use back_handler_platform::{AndroidBackError, AndroidBackHandler, AndroidBackStrategy};
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
