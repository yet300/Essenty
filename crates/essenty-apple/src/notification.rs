//! Shared Objective-C notification registration for Apple application hosts.

use objc2::runtime::{AnyObject, Sel};
use objc2_foundation::{NSNotificationCenter, NSNotificationName};

/// Failure to attach an application lifecycle observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationLifecycleError {
    /// Application notifications must be observed from the main thread.
    NotMainThread,
}

impl std::fmt::Display for ApplicationLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("application lifecycle must be created on the main thread")
    }
}

impl std::error::Error for ApplicationLifecycleError {}

/// Registers a platform's notification-to-selector mapping.
///
/// # Safety
/// Every selector must be implemented by `observer` with one
/// `NSNotification` argument. The observer must remain alive until all
/// registrations are removed.
pub(crate) unsafe fn register_notifications(
    center: &NSNotificationCenter,
    observer: &AnyObject,
    mappings: &[(Sel, &NSNotificationName)],
) {
    for (selector, name) in mappings {
        // SAFETY: The caller validates every mapping and owns the observer.
        unsafe { center.addObserver_selector_name_object(observer, *selector, Some(name), None) };
    }
}
