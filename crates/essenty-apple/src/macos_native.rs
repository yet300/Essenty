//! macOS application lifecycle observation. All callbacks stay on `AppKit`'s
//! main thread; the core registry deliberately remains `!Send`.

use crate::ApplicationLifecycleError;
use crate::notification::register_notifications;
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

        // SAFETY: The observer implements each listed selector with an
        // NSNotification argument and is unregistered before release.
        unsafe {
            register_notifications(
                &center,
                &observer,
                &[
                    (sel!(essentyDidFinishLaunching:), NSApplicationDidFinishLaunchingNotification),
                    (sel!(essentyDidBecomeActive:), NSApplicationDidBecomeActiveNotification),
                    (sel!(essentyDidResignActive:), NSApplicationDidResignActiveNotification),
                    (sel!(essentyDidHide:), NSApplicationDidHideNotification),
                    (sel!(essentyDidUnhide:), NSApplicationDidUnhideNotification),
                    (sel!(essentyWillTerminate:), NSApplicationWillTerminateNotification),
                ],
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
