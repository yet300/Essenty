//! watchOS `WatchKit` application-notification observation.

use crate::ApplicationLifecycleError;
use crate::notification::register_notifications;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSNotificationName, NSObject, NSObjectProtocol,
};

// These notification constants are exported by WatchKit. The constants are
// retained Foundation strings; the WatchKit framework owns their lifetime.
#[link(name = "WatchKit", kind = "framework")]
unsafe extern "C" {
    static WKApplicationDidFinishLaunchingNotification: &'static NSNotificationName;
    static WKApplicationWillEnterForegroundNotification: &'static NSNotificationName;
    static WKApplicationDidBecomeActiveNotification: &'static NSNotificationName;
    static WKApplicationWillResignActiveNotification: &'static NSNotificationName;
    static WKApplicationDidEnterBackgroundNotification: &'static NSNotificationName;
}

#[derive(Debug)]
struct Ivars {
    registry: LifecycleRegistry,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. WatchKit delivers
    // application notifications on the main thread, as required by the
    // registry and this observer's MainThreadOnly declaration.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    #[derive(Debug)]
    struct LifecycleObserver;

    impl LifecycleObserver {
        #[unsafe(method(essentyWatchDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Created);
        }

        #[unsafe(method(essentyWatchWillEnterForeground:))]
        fn will_enter_foreground(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyWatchDidBecomeActive:))]
        fn did_become_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Resumed);
        }

        #[unsafe(method(essentyWatchWillResignActive:))]
        fn will_resign_active(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Started);
        }

        #[unsafe(method(essentyWatchDidEnterBackground:))]
        fn did_enter_background(&self, _notification: &NSNotification) {
            let _ = self.ivars().registry.move_to(LifecycleState::Created);
        }
    }

    unsafe impl NSObjectProtocol for LifecycleObserver {}
);

impl LifecycleObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { registry: LifecycleRegistry::new() });
        // SAFETY: The NSObject initializer is valid and ivars are initialized.
        unsafe { msg_send![super(this), init] }
    }
}

/// Observes process-wide `WatchKit` notifications until dropped.
#[derive(Debug)]
pub struct ApplicationLifecycle {
    observer: Retained<LifecycleObserver>,
    center: Retained<NSNotificationCenter>,
}

impl ApplicationLifecycle {
    /// Attaches to `WatchKit` application notifications.
    ///
    /// # Errors
    /// Returns [`ApplicationLifecycleError::NotMainThread`] off the main thread.
    pub fn new() -> Result<Self, ApplicationLifecycleError> {
        let mtm = MainThreadMarker::new().ok_or(ApplicationLifecycleError::NotMainThread)?;
        let observer = LifecycleObserver::new(mtm);
        let center = NSNotificationCenter::defaultCenter();

        // SAFETY: The observer implements all listed selectors with an
        // NSNotification argument and is unregistered before release.
        unsafe {
            register_notifications(
                &center,
                &observer,
                &[
                    (
                        sel!(essentyWatchDidFinishLaunching:),
                        WKApplicationDidFinishLaunchingNotification,
                    ),
                    (
                        sel!(essentyWatchWillEnterForeground:),
                        WKApplicationWillEnterForegroundNotification,
                    ),
                    (sel!(essentyWatchDidBecomeActive:), WKApplicationDidBecomeActiveNotification),
                    (
                        sel!(essentyWatchWillResignActive:),
                        WKApplicationWillResignActiveNotification,
                    ),
                    (
                        sel!(essentyWatchDidEnterBackground:),
                        WKApplicationDidEnterBackgroundNotification,
                    ),
                ],
            );
        }

        // WKApplicationState is an NSInteger enum: Active = 0, Inactive = 1,
        // Background = 2. Reading it after registration covers observers
        // attached after the launch notification was posted.
        // SAFETY: WKApplication is a WatchKit main-thread-only application
        // singleton; this function already holds MainThreadMarker.
        let app: *mut AnyObject =
            unsafe { msg_send![objc2::class!(WKApplication), sharedApplication] };
        let app_state: isize = unsafe { msg_send![app, applicationState] };
        let target = match app_state {
            0 => LifecycleState::Resumed,
            1 => LifecycleState::Started,
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
        // SAFETY: This is the same observer registered in `new`.
        unsafe { self.center.removeObserver(&self.observer as &AnyObject) };
        let _ = self.observer.ivars().registry.destroy();
    }
}
