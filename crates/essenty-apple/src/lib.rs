//! Apple-family adapter for the Essenty Rust runtime.
//!
//! One crate covers the whole Apple family — iOS, macOS, watchOS, tvOS,
//! visionOS, and Mac Catalyst — with maximally shared implementation. Only
//! genuinely OS-specific behavior gets its own module, gated with
//! `cfg(target_vendor = "apple")` plus OS checks where required.
//!
//! Platform-specific [`ApplicationLifecycle`] observers use `objc2` to follow
//! `AppKit`, `UIKit`, or `WatchKit` application notifications. The shared
//! [`AppleLifecycle`] mapping remains available on every target.
//!
//! # Example
//!
//! ```rust
//! use essenty_apple::AppleLifecycle;
//!
//! let host = AppleLifecycle::new();
//! host.scene_connected();
//! host.scene_became_active();
//! assert!(host.registry().state().is_resumed());
//! ```

mod lifecycle;
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos",
    target_os = "watchos"
))]
mod notification;
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos",
    target_os = "watchos"
))]
pub use notification::ApplicationLifecycleError;
pub mod shared;
pub use lifecycle::AppleLifecycle;

#[cfg(target_os = "macos")]
mod macos_native;

#[cfg(target_os = "macos")]
pub use macos_native::ApplicationLifecycle;

#[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
mod uikit_native;

#[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
pub use uikit_native::ApplicationLifecycle;

#[cfg(target_os = "watchos")]
mod watch_native;

#[cfg(target_os = "watchos")]
pub use watch_native::ApplicationLifecycle;

#[cfg(test)]
mod tests;
