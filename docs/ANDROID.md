# Android integration

`essenty-android` is the Rust-native Android backend. It uses `android-activity`
with `NativeActivity`, native saved-state bytes, and direct Android platform
APIs. The consumer writes its host and callbacks in Rust; it does not need an
AndroidX dependency, a custom Gradle library module, consumer JNI, or a JVM
callback implementation. `jni-min-helper` embeds its own private proxy support.

| Capability | Implementation | Origin | Verification |
|---|---|---|---|
| Lifecycle | `NativeActivityLifecycle` over `android-activity` events | Rust backend for Android Essenty semantics | Android compile checks; no emulator lifecycle run in this integration |
| StateKeeper | `NativeActivityState` plus versioned binary envelope | Rust backend for Android Essenty semantics | Host codec tests and Android compile checks |
| InstanceKeeper | Not implemented | Requires a proven recreation ownership model | Blocked; see retained instances below |
| Back below API 33 | NativeActivity `KEYCODE_BACK` | Rust backend for Android Essenty semantics | Compile checked; runtime check unavailable |
| Back on API 33 | `OnBackInvokedCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | Interface path implemented; API 33 emulator unavailable |
| Back on API 34+ | `OnBackAnimationCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | Integrated path compile checked; earlier API 36 PoC runtime verified |

## Back handling

`AndroidBackHandler::attach` selects its platform strategy from the runtime API
level. API 34 and later register `OnBackAnimationCallback`; API 33 registers
`OnBackInvokedCallback`; older releases keep the existing NativeActivity key
route. Only one route owns back handling at a time. Modern callbacks are
registered while the Rust dispatcher has an enabled handler. After adding or
removing handlers through `dispatcher_mut`, call
`synchronize_enabled_state`; disabling during an owned predictive gesture is
deferred until invoke or cancel.

The Java callback only translates platform data and queues it. The Rust host
must call `drain_callbacks` on its event-loop thread to deliver events through
the core `BackDispatcher`. Gesture selection and priority remain exclusively in
the core. The API 34+ mapping preserves progress, edge, and touch coordinates.

Registration and unregistration are posted through
`AndroidApp::run_on_java_main_thread`. A setup or teardown error can be read
with `take_error`. The adapter owns the proxy and dispatcher global reference;
unregister runs before those references are released. Its JNI callback catches
Rust panics and clears a pending Java exception after callback failures.

```toml
[dependencies]
essenty-android = { version = "0.1", features = ["native-activity"] }
android-activity = { version = "0.6", features = ["native-activity"] }
```

```rust,ignore
use essenty_android::{AndroidBackHandler, NativeActivityLifecycle};
use essenty_back_handler::BackDispatcher;
use std::time::Duration;

#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    let host = NativeActivityLifecycle::new(app.clone());
    let mut back = AndroidBackHandler::attach(&app, BackDispatcher::new());
    let _back_handler = back.dispatcher_mut().register(0, true, |event| {
        // Handle the Essenty back event in Rust.
        let _ = event;
    });
    back.synchronize_enabled_state();
    loop {
        let mut destroyed = false;
        host.poll_events(Some(Duration::from_millis(50)), |event| match event {
            android_activity::PollEvent::Main(android_activity::MainEvent::InputAvailable) => {
                host.handle_input_events(back.dispatcher_mut(), |_| {
                    android_activity::InputStatus::Unhandled
                }).unwrap();
            }
            android_activity::PollEvent::Main(android_activity::MainEvent::Destroy) => {
                destroyed = true;
            }
            _ => {}
        });
        back.drain_callbacks();
        if destroyed { break; }
    }
    back.close().unwrap();
}
```

The crate does not package an APK or manifest. The application still chooses a
Rust Android packaging tool and declares its `NativeActivity` entry point.

## StateKeeper

`NativeActivityState` receives restored bytes through `StateLoader` and returns
saved bytes for `StateSaver`. The envelope uses a version byte, CRC-32 integrity
check, explicit unsupported-version errors, and a 512 KiB size cap. Empty state is
valid. The decoder still reads the prior `EST1` envelope so upgrades preserve
existing saved values. Corrupt or oversized state returns a typed error rather
than panicking. Unconsumed restored keys survive another save through the core
`StateKeeper` behavior.

Configuration recreation and process restoration both rely on Android
delivering saved-state bytes; the codec tests simulate restore/save but no
emulator lifecycle or process-relaunch test was available in this environment.

## Retained instances

The NativeActivity `InstanceKeeper` adapter remains unimplemented. Core
`InstanceKeeper` owns values through `Rc`, so a process-global registry cannot
be moved safely across arbitrary NativeActivity threads. A saved token alone
cannot prove that the replacement Activity will attach, and a stale registry
entry after failed recreation has no reliable native cleanup callback. Using
AndroidX `ViewModelStore` would violate the selected architecture. The adapter
will not claim retained-instance parity until thread ownership and stale-entry
cleanup have a deterministic, testable handoff.

## Lifecycle semantics

`NativeActivityLifecycle` maps the NativeActivity event loop to the core
`LifecycleRegistry` and leaves event polling with the Rust host. This is an
Essenty semantic adapter, not an AndroidX `LifecycleOwner` attachment. The
NativeActivity host observes its own event loop and does not synthesize AndroidX
attachment ordering. See [semantic compatibility](SEMANTIC_COMPATIBILITY.md).

## Verification boundary

- Host tests cover the state envelope, legacy format, corruption, version,
  empty state, and preservation of unconsumed values.
- Both `aarch64-linux-android` and `x86_64-linux-android` compile with the
  `native-activity` feature.
- The earlier API 36 PoC exercised start, progress, cancel, invoke, invoke-only
  callback selection, unregister, and one recreation. Constructed `BackEvent`
  values verified progress, swipe edge, and touch coordinates.
- API 33 runtime behavior, API 34 runtime behavior of the integrated crate,
  real varied gesture progress, 50–100 recreation stress, StateKeeper emulator
  recreation, and InstanceKeeper are not verified. No `adb` or emulator binary
  was available on PATH for this run.

The dynamic proxy callback count is exposed for debug/test instrumentation. The
PoC's callback counts were observations, not a JNI performance benchmark.
