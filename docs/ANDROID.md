# Android integration

`essenty-android` is the Rust-native Android backend. It uses `android-activity`
with `NativeActivity`, native saved-state bytes, and direct Android platform
APIs. The consumer writes its host and callbacks in Rust; it does not need an
AndroidX dependency, a custom Gradle library module, consumer JNI, or a JVM
callback implementation. `jni-min-helper` embeds its own private proxy support.

| Capability | Implementation | Origin | Verification |
|---|---|---|---|
| Lifecycle | `NativeActivityLifecycle` over `android-activity` events | Rust backend for Android Essenty semantics | Compile checked and API 36.1 emulator lifecycle events exercised |
| StateKeeper | `NativeActivityState` plus versioned binary envelope | Rust backend for Android Essenty semantics | Host codec tests, API 36.1 recreation restores a saved marker, and API 36.1 process-death relaunch restores the marker |
| InstanceKeeper core | Existing local `Rc` keeper | Shared core semantics | Implemented; no core ownership change |
| NativeActivity configuration retention | `MainEvent::ConfigChanged`; app declares handled changes | Rust-native Activity host model | Runtime verified on API 36.1: rotation, locale, night mode, resizing, and 50 events without recreation |
| Arbitrary Activity recreation retention | No cross-thread transfer of retained `Rc` values | Unsupported for non-`Send` retained values | Explicitly unsupported; use `StateKeeper` for serializable state |
| Host configuration diagnostic | `native_config::inspect_host_configuration` via `PackageManager` | Rust-only diagnostic | Host unit-tested; live-device read is implemented and compiles for both Android ABIs, readout on device pending |
| Back below API 33 | NativeActivity `KEYCODE_BACK` | Rust backend for Android Essenty semantics | Compile checked; no pre-33 emulator image available in this environment |
| Back on API 33 | `OnBackInvokedCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | Compile checked; API 33 system image download stalled, no AVD provisioned |
| Back on API 34+ | `OnBackAnimationCallback` through `DynamicProxy` | Rust backend for Android Essenty semantics | API 36.1 integrated adapter: attach, started/progressed/cancelled, committed invoke finishing the host, unregister-on-teardown, and no Rust dispatch after destroy |

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
unregister runs before those references are released. `attach` plus explicit
`close` is the reviewed pre-1.0 shape: `close` gives synchronous,
error-reporting teardown while `Drop` keeps a fire-and-forget uninstall as the
safe fallback (it cannot report errors and must not block gesture teardown).
No rename or RAII-only redesign is planned. Dropping during an owned
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

Declare only the changes the Rust UI actually handles. Claiming a category
means the app refreshes itself from the new configuration; a declared but
ignored category silently keeps stale layout, theme, or locale. The
recommendation is therefore split. The Rust source of truth is
`essenty_android::native_config::NativeConfigCategory`; the strings below
must match `baseline_manifest_value()` and `extended_manifest_value()`, which
a unit test pins.

Baseline (commonly required for a self-drawn Rust UI):

```toml
[[package.metadata.android.application.activity]]
config_changes = "orientation|screenSize|smallestScreenSize|screenLayout|uiMode|density|fontScale|locale|layoutDirection"
```

Extended (opt in only when the app reacts to the new state):

```toml
[[package.metadata.android.application.activity]]
config_changes = "orientation|screenSize|smallestScreenSize|screenLayout|uiMode|density|fontScale|locale|layoutDirection|keyboard|keyboardHidden|navigation|touchscreen|mcc|mnc|colorMode|fontWeightAdjustment|grammaticalGender"
```

The extended string is baseline plus the opt-in set, so adopting it later is
additive. Older Android versions ignore manifest values they do not know, so
declaring newer categories is harmless on them; it simply does not prevent
recreation there.

| Manifest category | Set | Since API | What changes | Rust host handling | If declared but ignored |
|---|---|---|---|---|---|
| `orientation` | baseline | 1 | Portrait/landscape rotation | `ConfigChanged`; re-read window size and configuration, re-lay out | Old layout on the new orientation |
| `screenSize` | baseline | 13 | Current screen size (rotation, fold, resize, multi-window) | `ConfigChanged`; redraw from current window/config | Stale layout |
| `smallestScreenSize` | baseline | 13 | Smallest width (fold, large resize) | `ConfigChanged`; re-evaluate size-class breakpoints | Previous size class kept |
| `screenLayout` | baseline | 4 | Layout class, including multi-window | `ConfigChanged`; re-read configuration | Missed layout-class transitions |
| `uiMode` | baseline | 8 | Night mode, desk/car dock | `ConfigChanged`; refresh theme-dependent colors/resources | Previous theme kept |
| `density` | baseline | 24 | Display density (scaling, moving across displays) | `ConfigChanged`; recompute pixel metrics | Wrong render scale. The `CONFIG_DENSITY` constant predates API 24; the manifest string is honored from API 24 |
| `fontScale` | baseline | 1 | System font scale | `ConfigChanged`; refresh text metrics and layout | Previous text size kept |
| `locale` | baseline | 1 | System language | `ConfigChanged`; reload Rust-localized strings. App-localized data is not updated automatically | Previous language kept |
| `layoutDirection` | baseline | 17 | LTR/RTL direction | `ConfigChanged`; mirror direction-dependent UI | Previous direction kept |
| `keyboard` | extended | 1 | Hardware keyboard type | Inspect input configuration if the UI adapts; otherwise nothing visible changes | Harmless but misleading |
| `keyboardHidden` | extended | 1 | Keyboard availability | Update keyboard-dependent layout if any | Harmless but misleading |
| `navigation` | extended | 1 | Navigation type | Inspect navigation configuration if the UI adapts | Harmless but misleading |
| `touchscreen` | extended | 1 | Touchscreen type | Relevant only when touch hardware changes | Harmless but misleading |
| `mcc`, `mnc` | extended | 1 | SIM country/network code | Refresh carrier-dependent data if any | Harmless but misleading |
| `colorMode` | extended | 26 | Wide gamut, HDR | Update color assumptions for color-sensitive rendering | Harmless but misleading |
| `fontWeightAdjustment` | extended | 31 | System font weight adjustment | Refresh typography | Previous weight kept |
| `grammaticalGender` | extended | 34 | Grammatical gender | Refresh localized text inflection | Previous inflection kept |

Size changes can also be treated as insignificant on some releases; then the
activity can stay alive and receive a callback even without the size flag.
Android 12 (API 31) and 12L (API 32) only deliver `onConfigurationChanged`
for significant changes; Android 17 (API 37) additionally keeps activities
alive by default for `keyboard`, `keyboardHidden`, `navigation`,
`touchscreen`, `colorMode`, and desk-related `uiMode` changes unless the app
opts back into recreation with `android:recreateOnConfigChanges`. The
explicit declaration above remains the consistent cross-version contract. See
[configuration-change guidance](https://developer.android.com/guide/topics/resources/runtime-changes).

`assetsPaths` and `resourcesUnused` are `ActivityInfo` bit constants, not
documented `android:configChanges` string values for the Activity element, and
are not part of either set. `resourcesUnused` is specifically unsafe for a
resource-using UI. Do not substitute `allKnown`: a future platform's
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

## Host configuration diagnostic

The host Activity's declared `ActivityInfo.configChanges` is readable from
Rust through `PackageManager`, with no Kotlin, JNI boilerplate, or logging
dependency imposed on the application:

```rust,ignore
use essenty_android::native_config::inspect_host_configuration;

let report = inspect_host_configuration(&app)?;
if !report.meets_baseline() {
    // One-shot, debug-time diagnostic: do not log on every config event.
    if let Some(warning) = report.retention_warning() {
        log::warn!("{warning}");
    }
}
```

`HostConfigurationReport` carries the declared set, the missing baseline and
extended categories, and any unknown bits from future platform values. A
missing flag is an unsupported retention configuration, not undefined
behavior: undeclared changes recreate the Activity, destroy the local keeper,
and restore only serialized state. `log_host_configuration_warning` is a thin
`log` convenience around the same check for hosts that already log. The live
device readout is implemented and compiles for both Android ABIs; the remaining
step is printing one report from a device run.

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
thread/keeper and the exact saved marker restored. A separate run verified
real process death: after backgrounding (57-byte saved envelope observed), the
process was killed with `adb shell am kill` — which preserves saved state,
unlike force-stop — and relaunching the Activity delivered the exact
`native-state-round-trip` marker bytes to a fresh `android_main` on a new
process, thread, and keeper.

## Retained instances

> **NativeActivity InstanceKeeper retention:** retained Rust object identity
> is preserved across configuration changes that the host Activity declares
> as handled through `android:configChanges`. The same `android_main`
> invocation, Rust thread, and local keeper stay alive; the app redraws from
> the updated configuration on `MainEvent::ConfigChanged`.

> **Actual Activity recreation:** if Android destroys and recreates the
> NativeActivity, arbitrary non-`Send` `Rc` retained instances are not
> transferred to the replacement thread. The old keeper is destroyed on its
> owning thread; the replacement `android_main` starts with a fresh keeper.
> This is expected behavior for this backend, not a bug.

The core `InstanceKeeper` owns values through local `Rc`; the Android host uses
that same keeper directly on the NativeActivity Rust thread. There is no
Android-specific wrapper, process-global keeper registry, `Rc` transfer, or
global `Rc`-to-`Arc` rewrite, and no new keeper type: the existing core
dispatcher already has the required lifetime, so the host only scopes it to
the `android_main` invocation. On `MainEvent::Destroy` the app destroys its
local keeper once and drops retained values on the owning Rust thread. Process
death ends all `InstanceKeeper` values by design. This differs from upstream
Android Essenty, which retains values in AndroidX `ViewModelStore` across
recreated Activities; the two are semantically equivalent only for declared,
in-place handled configuration changes. Applications should put
process-restorable values in `StateKeeper` or persistent storage.

## Lifecycle semantics

`NativeActivityLifecycle` maps the NativeActivity event loop to the core
`LifecycleRegistry` and leaves event polling with the Rust host. This is an
Essenty semantic adapter, not an AndroidX `LifecycleOwner` attachment. The
NativeActivity host observes its own event loop and does not synthesize AndroidX
attachment ordering. Handled `ConfigChanged` events do not touch the Essenty
lifecycle; only start/pause/resume/stop/destroy move it, and final Activity
destruction maps to `Destroyed`. See [semantic compatibility](SEMANTIC_COMPATIBILITY.md).

## Rust-only usage

A Rust developer needs no JNI, Java proxies, DEX, AndroidX, or Kotlin
knowledge to use the four primitives. One `android_main` owns them all; the
keeper lives as long as the host invocation:

```rust,ignore
use android_activity::{MainEvent, PollEvent};
use essenty_android::{AndroidBackHandler, NativeActivityLifecycle, NativeActivityState};
use essenty_back_handler::BackDispatcher;
use essenty_instance_keeper::InstanceKeeper;
use std::time::Duration;

#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    let host = NativeActivityLifecycle::new(app.clone());
    let lifecycle = host.registry();
    let mut keeper = InstanceKeeper::new();
    let mut back = AndroidBackHandler::attach(&app, BackDispatcher::new());
    let mut state: Option<NativeActivityState> = None;

    // One-shot debug diagnostic; never on the hot path.
    if let Ok(report) = essenty_android::native_config::inspect_host_configuration(&app) {
        essenty_android::native_config::log_host_configuration_warning(&report);
    }

    let mut destroyed = false;
    while !destroyed {
        host.poll_events(Some(Duration::from_millis(50)), |event| match event {
            PollEvent::Main(MainEvent::Resume { loader, .. }) if state.is_none() => {
                state = Some(NativeActivityState::from_loader(&loader).unwrap());
            }
            PollEvent::Main(MainEvent::SaveState { saver, .. }) => {
                let snapshot = state.as_ref().unwrap().save_bytes().unwrap();
                NativeActivityState::store(&saver, &snapshot);
            }
            PollEvent::Main(MainEvent::ConfigChanged { .. }) => {
                // Redraw from app.config(); keeper and lifecycle are untouched.
            }
            PollEvent::Main(MainEvent::Destroy) => destroyed = true,
            _ => {}
        });
        back.drain_callbacks();
    }
    back.close().unwrap();
    keeper.destroy_all();
}
```

The primitives stay independently usable: nothing forces them through a
single aggregate host object. A future Decompose-rs layer can receive the
`LifecycleRegistry`, `StateKeeper`, `InstanceKeeper`, and `BackDispatcher`
values directly, without Android or JNI types leaking into its core.

## Back registration synchronization

After changing registrations through `dispatcher_mut`, the caller calls
`synchronize_enabled_state()` so platform registration follows whether Rust
has an enabled handler. Explicit synchronization is deliberate: the core
dispatcher has no enabled-change listener, and automatically unregistering
mid-gesture would be unsafe — a disable requested during a claimed gesture is
instead deferred until that gesture receives invoke or cancel. `drain_callbacks`
also reconciles state after each batch, so the common event-loop shape above
stays correct without extra calls.

## Packaging with Cargo metadata

The application package (not the `essenty-android` dependency) declares its
`NativeActivity` entry. Verified with `cargo-apk2`, whose `config_changes`
field generates the manifest declaration without a hand-maintained manifest:

```toml
[lib]
crate-type = ["cdylib"]

[package.metadata.android]
package = "dev.example.app"
build_targets = ["aarch64-linux-android"]

[package.metadata.android.sdk]
min_sdk_version = 23
target_sdk_version = 37

[[package.metadata.android.application.activity]]
name = "android.app.NativeActivity"
label = "Example"
exported = true
config_changes = "orientation|screenSize|smallestScreenSize|screenLayout|uiMode|density|fontScale|locale|layoutDirection"

[[package.metadata.android.application.activity.meta_data]]
name = "android.app.lib_name"
value = "example"

[[package.metadata.android.application.activity.intent_filter]]
actions = ["android.intent.action.MAIN"]
categories = ["android.intent.category.LAUNCHER"]
```

Use the array-of-tables form (`[[...activity]]`) shown here; the singular
table form (`[...]`) is accepted by some tool versions for a single activity.
`cargo-apk` (rust-mobile) shares the `[package.metadata.android]` root but its
activity table shape varies by version — consult that tool's docs rather than
assuming identical syntax. The harness build proves the `cargo-apk2` form above
produces a working manifest with the exact `configChanges` string.

## Minimum supported API

The tested floor is `min_sdk_version = 23`, used by the runtime harness. All
modern back paths are runtime-selected (`PredictiveCallback` on API 34+,
`InvokedCallback` on API 33, `NativeKey` below), so older platforms use the
key fallback instead of raising the minimum. Lower API levels are untested;
do not lower the floor without a runtime check.

## Platform semantic matrix

| Essenty semantic | Android implementation | Limitation |
|---|---|---|
| Lifecycle | NativeActivity event loop via `android-activity` | No AndroidX owner attachment timing |
| StateKeeper | Native opaque saved-state bytes, versioned envelope | Restores only serialized bytes, on recreation or process relaunch |
| InstanceKeeper | Same-thread local keeper across handled config changes | Identity lost on actual Activity recreation and on process death, by design |
| BackHandler | Direct `android.window` platform callbacks | Shell-injected gestures report `progress == 0.0`; natural gesture curves need interactive verification |

## Error handling, unsafe, and binary notes

Setup and teardown failures surface as typed `AndroidBackError` values
readable via `take_error` (async registration) or returned from `close`;
state failures surface as `NativeStateError` or `NativeConfigError`. No
message is a bare "JNI failed": each carries the failing operation plus the
platform detail. No panic crosses an FFI boundary: the Java proxy callback
catches Rust panics and clears a pending Java exception after failures.

Unsafe blocks are confined to the Android-only back platform adapter and the
host configuration reader. Each documents its invariant: the `JavaVM` pointer
comes from `android-activity` and is only used to attach the calling thread;
the Activity raw pointer is wrapped in a local that never escapes its closure;
the proxy and dispatcher `GlobalRef`s are owned until unregister succeeds, and
a failed unregister quarantines the registration for retry instead of freeing
live callback state.

Binary baseline (debug, unoptimized): the harness APK totals ~40.2 MB, of
which ~40.2 MB is the debug Rust `.so` and `classes.dex` is 612 bytes — the
proxy/DEX support ships inside the existing library with no consumer-visible
JVM code beyond the NativeActivity shim. No with/without split was measured;
treat this as a baseline, not a regression gate.

## Verification boundary

- Host tests cover the state envelope, legacy format, corruption, version,
  empty state, preservation of unconsumed values, the config-category model
  (baseline/extended partition, manifest round-trip, warning content), and
  back bridge mapping.
- Both `aarch64-linux-android` and `x86_64-linux-android` compile with the
  `native-activity` feature; clippy is clean for host and both ABIs.
- API 36.1 runtime harness exercised lifecycle start/resume/destroy,
  portrait/landscape rotation handled in place with unchanged thread, keeper,
  and retained `Rc` identity, plus locale, UI night mode, and display resize
  from the earlier run and 50 consecutive configuration changes with one
  final destruction/drop.
- API 36.1 process-death run: backgrounding saved a 57-byte envelope,
  `adb shell am kill` terminated the process, and relaunch delivered the
  exact marker bytes to a fresh process, thread, and keeper.
- API 36.1 integrated back adapter: attach, a short swipe producing start,
  five progressed callbacks (progress `0.0`, left edge, touch coordinates),
  and cancel; a long swipe committing to invoke, which finished the host;
  teardown destroyed the keeper exactly once with no crash, and a
  post-destroy swipe produced zero Rust dispatches.
- Not exercised: API 33 runtime (image download stalled for 10 minutes with
  zero bytes; no AVD provisioned), API 34 phone runtime (only the XR headset
  image is installed), pre-33 runtime (no runnable image; empty stubs only),
  naturally varying gesture progress (shell input reports `0.0`; constructed
  `BackEvent` mapping stays verified), and a dedicated 50–100 gesture-cycle
  stress (the harness exits on invoke; 50 config events showed one stable
  back attachment and observer count).

## CI strategy

The cross-target compile matrix (all 21 targets plus both Android ABIs with
the `native-activity` feature) stays fast, non-flaky, and mandatory. Emulator
validation remains manual and periodic: one modern-API smoke run covering
launch, rotation, background/kill/relaunch, and a cancel plus commit back
gesture, using the `experiments/android-instance-keeper` harness procedure.
No multi-emulator CI matrix is added — it would be slow and flaky for gesture
and process-lifecycle timing. Compile-only checks cover the API matrix; deeper
emulator runs happen on demand for back-adapter or lifecycle changes.

The dynamic proxy callback count is exposed for debug/test instrumentation. The
PoC's callback counts were observations, not a JNI performance benchmark.
