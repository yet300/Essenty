//! `UIKit` application notifications for iOS, tvOS, visionOS and Catalyst.

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

/// Failure to attach a `UIKit` application observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationLifecycleError {
    /// `UIKit` application notifications must be observed from the main thread.
    NotMainThread,
}

impl std::fmt::Display for ApplicationLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("application lifecycle must be created on the main thread")
    }
}

impl std::error::Error for ApplicationLifecycleError {}

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

        // SAFETY: The selectors below exist with NSNotification parameters.
        // UIKit posts application notifications on the main thread. Drop
        // unregisters the observer before releasing it.
        unsafe {
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidFinishLaunching:),
                Some(UIApplicationDidFinishLaunchingNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyWillEnterForeground:),
                Some(UIApplicationWillEnterForegroundNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidBecomeActive:),
                Some(UIApplicationDidBecomeActiveNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyWillResignActive:),
                Some(UIApplicationWillResignActiveNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyDidEnterBackground:),
                Some(UIApplicationDidEnterBackgroundNotification),
                None,
            );
            center.addObserver_selector_name_object(
                &observer,
                sel!(essentyWillTerminate:),
                Some(UIApplicationWillTerminateNotification),
                None,
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
