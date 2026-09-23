//! macOS smoke test for actual Objective-C notification delivery.
//! Run with `cargo run -p essenty-apple --example macos_notifications`.

#[cfg(target_os = "macos")]
fn main() {
    use essenty_apple::ApplicationLifecycle;
    use essenty_lifecycle::LifecycleState;
    use objc2_app_kit::{
        NSApplicationDidBecomeActiveNotification, NSApplicationDidFinishLaunchingNotification,
        NSApplicationDidHideNotification, NSApplicationDidResignActiveNotification,
        NSApplicationDidUnhideNotification,
    };
    use objc2_foundation::NSNotificationCenter;

    let lifecycle = ApplicationLifecycle::new().expect("main thread");
    let center = NSNotificationCenter::defaultCenter();

    // SAFETY: These are valid AppKit notification names and no object is
    // required for application-wide notification delivery.
    unsafe {
        center.postNotificationName_object(NSApplicationDidFinishLaunchingNotification, None);
        center.postNotificationName_object(NSApplicationDidBecomeActiveNotification, None);
    }
    assert_eq!(lifecycle.registry().state(), LifecycleState::Resumed);

    // SAFETY: Same notification center and a valid AppKit notification name.
    unsafe { center.postNotificationName_object(NSApplicationDidResignActiveNotification, None) };
    assert_eq!(lifecycle.registry().state(), LifecycleState::Started);

    // SAFETY: Same notification center and valid AppKit notification names.
    unsafe {
        center.postNotificationName_object(NSApplicationDidHideNotification, None);
    }
    assert_eq!(lifecycle.registry().state(), LifecycleState::Created);
    // SAFETY: Same notification center and valid AppKit notification names.
    unsafe { center.postNotificationName_object(NSApplicationDidUnhideNotification, None) };
    assert_eq!(lifecycle.registry().state(), LifecycleState::Started);

    let detached = ApplicationLifecycle::new().expect("main thread");
    let registry = detached.registry().clone();
    drop(detached);
    let before = registry.state();
    // SAFETY: Same valid notification name. The dropped observer must not
    // receive this synchronous notification.
    unsafe { center.postNotificationName_object(NSApplicationDidBecomeActiveNotification, None) };
    assert_eq!(registry.state(), before);
}

#[cfg(not(target_os = "macos"))]
fn main() {}
