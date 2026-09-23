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

`NativeActivityState` connects the core state keeper to the
`android-activity` NativeActivity host. Create it from the `Resume` event's
`StateLoader`, register providers and consume restored entries through
`keeper_mut()`, then call `save_bytes()` and pass those bytes to the
`SaveState` event's `StateSaver`. The versioned container stores each key's
opaque bytes without imposing a serialization format. Corrupt containers
return `NativeStateError` rather than silently dropping restored state.

```rust,ignore
use android_activity::{MainEvent, PollEvent};
use essenty_android::NativeActivityState;

let mut state = None;
let mut snapshot = Vec::new();
host.poll_events(None, |event| match event {
    PollEvent::Main(MainEvent::Resume { loader, .. }) => {
        if state.is_none() {
            state = Some(NativeActivityState::from_loader(&loader).unwrap());
        }
    }
    PollEvent::Main(MainEvent::SaveState { saver, .. }) => {
        snapshot = state.as_ref().unwrap().save_bytes().unwrap();
        NativeActivityState::store(&saver, &snapshot);
    }
    _ => {}
});
```

## Verified

- Core and mapping helper unit tests: macOS host test run.
- `NativeActivityLifecycle`: `cargo check` for `aarch64-linux-android`
  and `x86_64-linux-android`.
- NativeActivity state container: host round-trip and corruption tests,
  Android target compilation.
- Emulator/device runtime: not tested.

## Planned

- A Rust-facing adapter for arbitrary AndroidX `LifecycleOwner`,
  `SavedStateRegistryOwner`, and `ViewModelStoreOwner` environments.
- AndroidX `SavedStateRegistryOwner` and `ViewModelStoreOwner` attachment.
- AndroidX `OnBackPressedDispatcher` registration, enabled-state observation,
  and predictive-back start/progress/cancel/commit. The current Back key path
  has no predictive progress.

AndroidX `OnBackPressedCallback` is an abstract Java class; its callbacks
cannot be implemented by a Rust closure through JNI alone. A future adapter
will need a private bundled shim or equivalent host glue. Cargo does not
inject such code or AndroidX dependencies into an arbitrary existing APK, so
fully automatic AndroidX attachment is not claimed. Any shim must be packaged
by supported Rust Android tooling and remain invisible to ordinary callers.
