//! Rust-native adapters for Android Essenty semantics.
//!
//! The optional `native-activity` feature integrates Lifecycle, `StateKeeper`,
//! and `BackHandler` with `android-activity` `NativeActivity` and direct platform
//! APIs. `AndroidX` is not part of the primary backend. `InstanceKeeper`
//! retention uses the existing local keeper on the `NativeActivity` Rust
//! thread: declared `android:configChanges` are handled in place (see
//! [`native_config`]), while actual Activity recreation ends the keeper and
//! restores only serialized state. Android/JNI dependencies stay inside this
//! crate and never enter core crates.
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
pub mod native_config;
mod native_state;
mod state_keeper;

pub use back_handler::AndroidBackBridge;
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use back_handler_platform::{AndroidBackError, AndroidBackHandler, AndroidBackStrategy};
pub use lifecycle::AndroidLifecycle;
pub use native_config::{
    HostConfigurationReport, ManifestParse, NativeConfigCategory, NativeConfigError,
    NativeConfigSet, parse_manifest_value,
};
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub use native_config::{inspect_host_configuration, log_host_configuration_warning};
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
