//! macOS application lifecycle observation. All callbacks stay on `AppKit`'s
//! main thread; the core registry deliberately remains `!Send`.

use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidFinishLaunchingNotification, NSApplicationDidHideNotification,
    NSApplicationDidResignActiveNotification, NSApplicationDidUnhideNotification,
    NSApplicationWillTerminateNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};

#[derive(Debug)]
struct Ivars {
    registry: LifecycleRegistry,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The observer is
    // confined to AppKit's main thread, as is its !Send registry.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    #[derive(Debug)]
    struct LifecycleObserver;

    impl LifecycleObserver {
        #[unsafe(method(essentyDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Created);
        }

        #[unsafe(method(essentyDidBecomeActive:))]
        fn did_become_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Resumed);
        }

        #[unsafe(method(essentyDidResignActive:))]
        fn did_resign_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyDidHide:))]
        fn did_hide(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Created);
        }

        #[unsafe(method(essentyDidUnhide:))]
        fn did_unhide(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyWillTerminate:))]
        fn will_terminate(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.destroy();
        }
    }

    unsafe impl NSObjectProtocol for LifecycleObserver {}
);

impl LifecycleObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { registry: LifecycleRegistry::new() });
        // SAFETY: The NSObject designated initializer is valid and all ivars
        // were installed before the object is initialized.
        unsafe { msg_send![super(this), init] }
    }
}

/// Failure to attach an application lifecycle observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationLifecycleError {
    /// `ApplicationLifecycle::new` must be called from `AppKit`'s main thread.
    NotMainThread,
}

impl std::fmt::Display for ApplicationLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("application lifecycle must be created on the main thread")
    }
}

impl std::error::Error for ApplicationLifecycleError {}

/// Automatically observes the current macOS application until dropped.
///
/// The registry is local to the main thread. Dropping this value unregisters
/// its Objective-C observer before the retained object is released.
#[derive(Debug)]
pub struct ApplicationLifecycle {
    observer: Retained<LifecycleObserver>,
    center: Retained<NSNotificationCenter>,
}

impl ApplicationLifecycle {
    /// Attaches to `NSApplication` notifications.
    ///
    /// # Errors
    /// Returns [`ApplicationLifecycleError::NotMainThread`] off the main thread.
    pub fn new() -> Result<Self, ApplicationLifecycleError> {
        let mtm = MainThreadMarker::new().ok_or(ApplicationLifecycleError::NotMainThread)?;
        let observer = LifecycleObserver::new(mtm);
        let center = NSNotificationCenter::defaultCenter();

        // SAFETY: Each selector is implemented by LifecycleObserver with an
        // NSNotification argument. AppKit posts these notifications on the
        // main thread, and Drop unregisters before releasing the observer.
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidFinishLaunching:),
                Some(NSApplicationDidFinishLaunchingNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidBecomeActive:),
                Some(NSApplicationDidBecomeActiveNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidResignActive:),
                Some(NSApplicationDidResignActiveNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidHide:),
                Some(NSApplicationDidHideNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidUnhide:),
                Some(NSApplicationDidUnhideNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyWillTerminate:),
                Some(NSApplicationWillTerminateNotification),
                None,
            );
        }

        let app = NSApplication::sharedApplication(mtm);
        if app.isRunning() {
            let target =
                if app.isActive() { LifecycleState::Resumed } else { LifecycleState::Started };
            let _ = observer.ivars().registry.move_to(target);
        }

        Ok(Self { observer, center })
    }

    /// The shared Rust lifecycle registry.
    #[must_use]
    pub fn registry(&self) -> &LifecycleRegistry {
        &self.observer.ivars().registry
    }
}

impl Drop for ApplicationLifecycle {
    fn drop(&mut self) {
        // SAFETY: This is the same live observer registered in `new`.
        unsafe { self.center.removeObserver(&self.observer as &AnyObject) };
    }
}
