# Android integration

## Implemented

The four core Essenty primitives work in Rust on Android. The optional
`essenty-android/native-activity` feature integrates with the Rust
`android-activity` **NativeActivity** host. Its `NativeActivityLifecycle`
wraps the host's event loop, drives `LifecycleRegistry` for start, resume,
pause, stop, and destroy, and routes the ordinary Back key through a core
`BackDispatcher`. Other events remain available to the Rust application.

```toml
[dependencies]
essenty = "0.1"
essenty-android = { version = "0.1", features = ["native-activity"] }
android-activity = { version = "0.6", features = ["native-activity"] }
```

```rust,ignore
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use essenty::back_handler::BackDispatcher;
use essenty_android::NativeActivityLifecycle;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let host = NativeActivityLifecycle::new(app);
    let mut back = BackDispatcher::new();
    loop {
        let mut done = false;
        host.poll_events(None, |event| match event {
            PollEvent::Main(MainEvent::InputAvailable) => {
                host.handle_input_events(&mut back, |_| InputStatus::Unhandled).unwrap();
            }
            PollEvent::Main(MainEvent::Destroy) => done = true,
            _ => {}
        });
        if done { break; }
    }
}
```

The `android-activity` dependency provides its own Android host glue. The
application still needs normal Android packaging with a NativeActivity entry
point; adding a Cargo dependency alone does not create an APK or manifest.
There is no application-written Java/Kotlin callback code in this path.

The host-testable `AndroidLifecycle`, `AndroidStateHost`, and
`AndroidBackBridge` remain mapping helpers. They do **not** automatically
connect to Android framework or AndroidX owners.

## Verified

- Core and mapping helper unit tests: macOS host test run.
- `NativeActivityLifecycle`: `cargo check` for `aarch64-linux-android`
  and `x86_64-linux-android`.
- Emulator/device runtime: not tested.

## Planned

- A Rust-facing adapter for arbitrary AndroidX `LifecycleOwner`,
  `SavedStateRegistryOwner`, and `ViewModelStoreOwner` environments.
- Automatic saved-state restoration and configuration-change retention for
  the Android host, preserving core byte format independence.
- AndroidX `OnBackPressedDispatcher` registration, enabled-state observation,
  and predictive-back start/progress/cancel/commit. The current Back key path
  has no predictive progress.

AndroidX `OnBackPressedCallback` is an abstract Java class; its callbacks
cannot be implemented by a Rust closure through JNI alone. A future adapter
will need a private bundled shim or equivalent host glue. Cargo does not
inject such code or AndroidX dependencies into an arbitrary existing APK, so
fully automatic AndroidX attachment is not claimed. Any shim must be packaged
by supported Rust Android tooling and remain invisible to ordinary callers.
