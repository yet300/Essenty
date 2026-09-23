//! `UIKit` application notifications for iOS, tvOS, visionOS and Catalyst.

use crate::ApplicationLifecycleError;
use crate::notification::register_notifications;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
use objc2_ui_kit::{
    UIApplication, UIApplicationDidBecomeActiveNotification,
    UIApplicationDidEnterBackgroundNotification, UIApplicationDidFinishLaunchingNotification,
    UIApplicationState, UIApplicationWillEnterForegroundNotification,
    UIApplicationWillResignActiveNotification, UIApplicationWillTerminateNotification,
};

#[derive(Debug)]
struct Ivars {
    registry: LifecycleRegistry,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. UIKit application
    // notifications and the !Send registry are confined to the main thread.
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

        #[unsafe(method(essentyWillEnterForeground:))]
        fn will_enter_foreground(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyDidBecomeActive:))]
        fn did_become_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Resumed);
        }

        #[unsafe(method(essentyWillResignActive:))]
        fn will_resign_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyDidEnterBackground:))]
        fn did_enter_background(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Created);
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
        // SAFETY: The NSObject designated initializer is valid; ivars exist.
        unsafe { msg_send![super(this), init] }
    }
}

/// Observes process-wide `UIKit` application notifications until dropped.
///
/// This is an application lifecycle, not an individual `UIScene` lifecycle.
#[derive(Debug)]
pub struct ApplicationLifecycle {
    observer: Retained<LifecycleObserver>,
    center: Retained<NSNotificationCenter>,
}

impl ApplicationLifecycle {
    /// Attaches to the current `UIKit` application.
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
                    (sel!(essentyDidFinishLaunching:), UIApplicationDidFinishLaunchingNotification),
                    (
                        sel!(essentyWillEnterForeground:),
                        UIApplicationWillEnterForegroundNotification,
                    ),
                    (sel!(essentyDidBecomeActive:), UIApplicationDidBecomeActiveNotification),
                    (sel!(essentyWillResignActive:), UIApplicationWillResignActiveNotification),
                    (sel!(essentyDidEnterBackground:), UIApplicationDidEnterBackgroundNotification),
                    (sel!(essentyWillTerminate:), UIApplicationWillTerminateNotification),
                ],
            );
        }

        let state = UIApplication::sharedApplication(mtm).applicationState();
        let target = match state {
            UIApplicationState::Active => LifecycleState::Resumed,
            UIApplicationState::Inactive => LifecycleState::Started,
            _ => LifecycleState::Created,
        };
        let _ = observer.ivars().registry.move_to(target);
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
        unsafe { self.center.removeObserver(&self.observer) };
    }
}
