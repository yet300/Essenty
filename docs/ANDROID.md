# Android integration

`essenty-android` is the Rust-native Android backend. It uses `android-activity`
with `NativeActivity`, native saved-state bytes, and direct Android platform
APIs. The consumer writes its host and callbacks in Rust; it does not need an
AndroidX dependency, a custom Gradle library module, consumer JNI, or a JVM
callback implementation. `jni-min-helper` embeds its own private proxy support.

| Capability | Implementation | Origin | Verification |
|---|---|---|---|
| Lifecycle | `NativeActivityLifecycle` over `android-activity` events | Rust backend for Android Essenty semantics | Compile checked and API 36.1 emulator lifecycle events exercised |
| StateKeeper | `NativeActivityState` plus versioned binary envelope | Rust backend for Android Essenty semantics | Host codec tests and API 36.1 unhandled-orientation recreation restored a saved marker; process relaunch not exercised |
| InstanceKeeper core | Existing local `Rc` keeper | Shared core semantics | Implemented; no core ownership change |
| NativeActivity configuration retention | `MainEvent::ConfigChanged`; app declares handled changes | Rust-native Activity host model | Runtime verified on API 36.1: rotation, locale, night mode, resizing, and 50 events without recreation |
| Arbitrary Activity recreation retention | No cross-thread transfer of retained `Rc` values | Unsupported for non-`Send` retained values | Explicitly unsupported; use `StateKeeper` for serializable state |
| Back below API 33 | NativeActivity `KEYCODE_BACK` | Rust backend for Android Essenty semantics | Compile checked; this run did not exercise a pre-33 emulator |
| Back on API 33 | `OnBackInvokedCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | Compile checked; API 33 emulator image unavailable |
| Back on API 34+ | `OnBackAnimationCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | API 36.1 integrated adapter attached, received a cancelled predictive gesture, and unregistered on close; invoke was not shell-gesture verified |

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
unregister runs before those references are released. Dropping during an owned
gesture defers unregister until the terminal platform callback. If Java rejects
unregister, the adapter keeps the proxy alive and retries on a later Android
host attach; that exceptional fallback can retain the old dispatcher until the
process exits if no later host appears. Its JNI callback catches Rust panics and
clears a pending Java exception after callback failures.

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

The crate does not package an APK or own application manifest metadata. The
application still chooses a Rust Android packaging tool and declares its
`NativeActivity` entry point. The root application package must also opt in to
the configuration changes it handles; dependency metadata from
`essenty-android` cannot alter the consuming APK's Activity declaration.

## Configuration changes and retained instances

Android normally destroys and recreates an Activity for a configuration change.
For every change the Activity handles in place, Android instead keeps the
NativeActivity running, updates its resource configuration, and calls
`onConfigurationChanged`. `android-activity` reports this as
`MainEvent::ConfigChanged`, and `AndroidApp::config()` exposes the updated NDK
configuration. The Rust `android_main` invocation, thread-local state, and its
existing `InstanceKeeper` therefore remain alive; no `Rc` crosses a thread
boundary. See Android's [`android:configChanges` reference](https://developer.android.com/guide/topics/manifest/activity-element#config) and
[`NativeActivity.onConfigurationChanged`](https://developer.android.com/reference/android/app/NativeActivity#onConfigurationChanged(android.content.res.Configuration)),
and [`android-activity::MainEvent`](https://docs.rs/android-activity/0.6.1/android_activity/enum.MainEvent.html).

For a Rust UI that can refresh itself from the new window and configuration
state, the recommended broad NativeActivity declaration is:

```toml
[package.metadata.android.application.activity]
config_changes = "orientation|screenSize|smallestScreenSize|screenLayout|uiMode|keyboard|keyboardHidden|navigation|touchscreen|locale|layoutDirection|mcc|mnc|density|fontScale|fontWeightAdjustment|colorMode|grammaticalGender"
```

This is Cargo metadata used by `cargo-apk` and `cargo-apk2`; it generates the
manifest declaration without a hand-maintained manifest. For `cargo-apk2`
versions/configurations that declare activities as an array, put the same field
under `[[package.metadata.android.application.activity]]`. `cargo-apk2` documents
Cargo metadata and NativeActivity packaging in its [manifest configuration
guide](https://github.com/mzdk100/cargo-apk2#manifest); `cargo-apk` uses the same
package metadata model. Keep this on the application package's manifest, not a
library dependency.

The recommendation covers the standard documented configuration categories,
including display geometry and resources, locale/direction, input hardware,
theme/color, font, and telephony configuration. The API introduction and Rust
event status are summarized below. Older Android versions ignore categories
they do not know; the installed platform SDK compiles the manifest values, and
the API 36.1 runtime exercised the listed changes available on that device.

| Manifest category | Platform availability | Rust/event handling | Runtime notes |
|---|---|---|---|
| `orientation` | API 1 | `ConfigChanged`; query `AndroidApp::config()` and window size | API 36.1 portrait/landscape tested |
| `screenSize`, `smallestScreenSize` | API 13 | `ConfigChanged`; redraw from current window/config | API 36.1 display resize tested; significant-size behavior varies by OS |
| `screenLayout` | API 4 | `ConfigChanged`; re-read configuration | Included for layout and multi-window changes |
| `uiMode` | API 8 | `ConfigChanged`; inspect UI mode | API 36.1 night/light changes tested |
| `keyboard`, `keyboardHidden`, `navigation`, `touchscreen` | API 1 | `ConfigChanged`; inspect input configuration as needed | Hardware/input changes are less common on the emulator |
| `locale`, `mcc`, `mnc` | API 1 | `ConfigChanged`; locale/resource configuration is available to Android; app-localized Rust data must be refreshed by the app | System locale change tested; MCC/MNC not available to force on this emulator |
| `layoutDirection` | API 17 | `ConfigChanged`; query locale/layout direction and update UI | Not separately forced |
| `density` | API 24 as a manifest value | `ConfigChanged`; query configuration and window metrics | Emulator density was observed; density-change transition not separately forced |
| `fontScale` | API 1 | `ConfigChanged`; refresh text/layout metrics | Font-scale transition not separately forced |
| `fontWeightAdjustment` | API 31 | `ConfigChanged`; app refreshes typography | Not separately forced |
| `colorMode` | API 26 | `ConfigChanged`; app updates color/HDR assumptions | Device lacked a practical color-mode transition |
| `grammaticalGender` | API 34 | `ConfigChanged`; app refreshes localized text | Not separately forced |

Android documents that an unhandled configuration still recreates the
Activity. Size changes can also be treated as insignificant on some releases;
then the activity can stay alive and receive a callback even without the size
flag. Handling configuration changes means Rust must update its own UI and
resource-dependent state; it does not make the old layout correct
automatically. Android 17/API 37 also changes default recreation behavior for
keyboard, keyboardHidden, navigation, touchscreen, colorMode, and desk-related
`uiMode` changes. The explicit declaration remains useful for a consistent
cross-version contract. See [configuration-change guidance](https://developer.android.com/guide/topics/resources/runtime-changes).

`assetsPaths` and `resourcesUnused` are newer `ActivityInfo` bit constants, not
documented `android:configChanges` string values for the Activity element, and
are not part of the recommendation. `resourcesUnused` is specifically unsafe
for a resource-using UI. Do not substitute `allKnown`: a future platform's
categories may require behavior the current app cannot handle.

In this model, configuration retention is semantically sufficient for the
verified Rust NativeActivity case: UI configuration changes do not need a new
Rust owner when the event loop can react in place. This differs from upstream
Android Essenty, which uses AndroidX `ViewModelStore` to retain instances while
ordinary Android Activities are recreated. The implementations are
semantically equivalent only for declared, in-place handled configuration
changes; arbitrary Activity recreation is not equivalent.

On `MainEvent::Destroy`, the app destroys its local keeper once and drops its
retained values on the owning Rust thread. Process death also ends all
`InstanceKeeper` values. `StateKeeper` serialized bytes are the restoration
mechanism; a new process creates a fresh keeper and restores only serialized
state. A system recreation caused by an unlisted configuration or another
system lifecycle event also creates a fresh `android_main`; the old keeper is
destroyed and arbitrary non-`Send` values cannot be transferred to it.

The Android platform exposes the host Activity's `ActivityInfo.configChanges`
through PackageManager/JNI, so a future debug validator is technically
possible. This backend currently does not expose one. A missing flag is an
unsupported retention configuration, not undefined behavior: the adapter does
not transfer values, and the application must rely on normal destruction plus
saved state. The test harness inspects the packaged manifest directly.

## StateKeeper

`NativeActivityState` receives restored bytes through `StateLoader` and returns
saved bytes for `StateSaver`. The envelope uses a version byte, CRC-32 integrity
check, explicit unsupported-version errors, and a 512 KiB size cap. Empty state is
valid. The decoder still reads the prior `EST1` envelope so upgrades preserve
existing saved values. Corrupt or oversized state returns a typed error rather
than panicking. Unconsumed restored keys survive another save through the core
`StateKeeper` behavior.

Configuration recreation and process restoration rely on Android delivering
saved-state bytes. The API 36.1 harness deliberately omitted `orientation` for
one run, registered a StateKeeper byte marker, rotated, observed
`MainEvent::SaveState`, then saw a second `android_main` invocation with a new
thread/keeper and the exact saved marker restored. This verifies the native
saved-state path across an actual Activity recreation. Process relaunch after
process death was not exercised.

## Retained instances

The core `InstanceKeeper` owns values through local `Rc`; the Android host uses
that same keeper directly on the NativeActivity Rust thread. It survives the
in-place configuration changes declared by the application because the
`android_main` invocation does not end. There is no Android-specific wrapper,
process-global keeper registry, `Rc` transfer, or global `Rc` to `Arc` rewrite.
The guarantee does not cover an Activity recreation. If Android sends
`MainEvent::Destroy`, the local keeper is destroyed normally. Applications
should put process-restorable values in `StateKeeper` or persistent storage.

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
- API 36.1 runtime harness exercised lifecycle start/resume/destroy, orientation,
  locale, UI night mode, display resize, and 50 consecutive configuration
  changes while retaining one actual non-`Send` `Rc` value. It observed one
  `android_main`, one keeper identity, one retained object identity, no destroy
  during the changes, then one final destruction/drop.
- The integrated API 36.1 back adapter attached, received a cancelled
  predictive-back gesture, and unregistered on close. The shell-driven gesture
  did not produce an invoke callback. No API 33 AVD was available, and no
  separate API 34 run was made; the earlier isolated API 36 PoC remains
  separate evidence.
- Process relaunch after process death, API <33 runtime back, and API 33 runtime
  back were not exercised. The emulator and `adb` were available at the
  configured SDK path; there was no API 33 AVD.

The dynamic proxy callback count is exposed for debug/test instrumentation. The
PoC's callback counts were observations, not a JNI performance benchmark.
