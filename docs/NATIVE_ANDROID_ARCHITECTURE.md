# Native Android backend decision and verification

## Decision

The primary Android backend is Rust `NativeActivity` plus direct platform APIs.
The consuming application remains Rust-only. Internal JVM machinery packaged
by `jni-min-helper` is acceptable; AndroidX and consumer-owned JVM callback
code are not required. The NativeActivity host keeps its Rust owner alive for
declared configuration changes by handling those changes in place. This lets
the existing local `InstanceKeeper` and its non-`Send` values remain on their
own thread.

The application root Cargo package must declare its Activity's
`config_changes`; a library dependency cannot inject a setting into the
consumer's generated APK manifest. The recommended value and support contract
are in [ANDROID.md](ANDROID.md#configuration-changes-and-retained-instances).

The API 34+ route registers `android.window.OnBackAnimationCallback` using the
crate's `DynamicProxy` adapter. API 33 uses
`android.window.OnBackInvokedCallback`. Below API 33, the NativeActivity key
path owns back handling. Rust `BackDispatcher` remains the only owner of
priority and predictive gesture semantics.

## PoC runtime evidence

The proof on branch `codex/android-predictive-proxy-poc` at commit `1887598`
ran on an API 36 emulator. It observed start, multiple progress callbacks,
cancel and invoke; also ordinary invoke-only back, main-thread registration and
unregistration, and one Activity recreation. Constructed Android `BackEvent`
objects verified progress, edge, and touch-coordinate transport. Real shell
gestures did not establish naturally varying progress values.

The production adapter has since moved into `crates/essenty-android`. The
integrated adapter was exercised on an API 36.1 emulator: attach, a short
swipe producing start/progressed/cancel, a long swipe committing to invoke
(which finished the host), teardown destroying the keeper exactly once with
no crash, and a post-destroy swipe producing zero Rust dispatches. API 33
runtime behavior remains unverified because the API 33 image download
stalled and no AVD was provisioned.

## Dependency decision

`jni-min-helper` 0.4.7 is isolated behind the Android-only `native-activity`
feature. It is dual licensed MIT OR Apache-2.0, supports `jni` 0.22, and
provides a Rust closure-backed dynamic proxy plus its private DEX invocation
handler. Its `DynamicProxy` documentation warns that dropping its Rust handler
while Java can still invoke non-void proxy methods is unsafe; therefore
registration owns the proxy until unregistration. `AndroidApp` schedules both
registration and unregistration on the Java main thread. The crate's APIs and
prebuilt DEX behavior are treated as replaceable internals, not public API.

The pinned 0.4.7 crate declares `MIT OR Apache-2.0` and supports `jni` 0.22.
Its source contains unsafe JNI/context operations, a process-global mutex for
handler closures, and a Java-to-Rust dispatch trampoline; the back adapter
contains those calls in one Android-only module. The build script compiles its
private Java handler to DEX and falls back to a prebuilt DEX if toolchain
compilation fails. The published repository and crate version are visible at
[`jni-min-helper`](https://github.com/uglyoldbob/jni-min-helper) and
[its 0.4.7 API docs](https://docs.rs/jni-min-helper/0.4.7/). Release cadence
does not establish a support guarantee, so keep this dependency behind the
adapter boundary.

The helper uses JNI, generated proxy support, and embedded DEX; it increases
APK contents compared with a purely native back path. There is no measured
APK size or callback-latency baseline yet. The adapter exposes a callback counter
for future profiling. No DEX rewrite or optimization is justified without
measurement.

## State and lifecycle

NativeActivity lifecycle remains driven by `android-activity` events and the
Rust host's event loop. Native saved state uses a versioned binary envelope,
CRC-32, a 512 KiB cap, a defined unsupported-version error, and legacy `EST1`
decoding. It has host corruption/round-trip tests and Android target compile
checks. The API 36.1 harness also forced one orientation recreation with a
StateKeeper marker: Android delivered a 57-byte saved envelope, the replacement
`android_main` ran with a new thread and keeper, and the exact marker bytes were
restored. A further run verified real process death: after backgrounding, `adb
shell am kill` terminated the process, and relaunch delivered the exact marker
bytes to a fresh process, thread, and keeper. Force-stop is intentionally not
used as a test because Android discards restoration state for it.

The adapter intentionally reports **Android Essenty semantic behavior**. It
does not claim implementation identity with upstream's AndroidX owners. The
NativeActivity lifecycle observes the NativeActivity event loop; state uses
native saved bytes; and predictive back uses direct `android.window` APIs.

## NativeActivity InstanceKeeper

Configuration changes declared in the consumer Activity's
`android:configChanges` are handled by the current NativeActivity. Android calls
`onConfigurationChanged` instead of destroying and recreating the Activity;
`android-activity` turns this into `MainEvent::ConfigChanged` and updates
`AndroidApp::config()`. The application handles the event and redraws from the
updated configuration. The same `android_main` invocation and Rust thread
remain alive, so the same local `InstanceKeeper` and `Rc` instances survive
without transfer.

The runtime harness retained a non-`Send` object containing `PhantomData<Rc<()>>`
through portrait/landscape/portrait rotation, locale, night mode and display
resize, then through 50 configuration events. It recorded one entry to
`android_main`, unchanged Rust thread and keeper identities, unchanged object
pointer/counter, and no `Destroy` until explicit finish. Final `Destroy` ran
`destroy_all` once and dropped the object once. This demonstrates the
configuration-retention path on API 36.1. It does not demonstrate arbitrary
Activity recreation.

This is the appropriate Rust-native model when the app can adapt in place. It
differs from the AndroidX path, where a recreated Activity uses
`ViewModelStore`; semantic equivalence applies only to the handled configuration
changes. For any unlisted change that causes recreation, or a system-initiated
Activity/process teardown, `android_main` ends and the local keeper is
destroyed. Non-`Send` values cannot move to a replacement thread. `StateKeeper`
restores serialized values; it does not restore `InstanceKeeper` objects.

There is no global registry and no change to the core `Rc` implementation. A
separate `Send + Sync` keeper could support applications that explicitly need
shared ownership across threads, but this task found no demonstrated Android
need for that variant. It would fragment the API and impose `Send`/`Sync`
constraints on retained components, unlike Essenty's local owner model. Do not
add it without a concrete application requirement and independent design.

The platform queries the hosting Activity's `ActivityInfo.configChanges`
through `PackageManager`/JNI in `essenty_android::native_config`. The
`NativeConfigCategory` model is the single source of truth for the baseline
and extended manifest strings; `inspect_host_configuration` returns a
`HostConfigurationReport` with declared, missing-baseline,
missing-extended, and unknown-bit sets plus a one-shot retention warning.
Missing flags reduce the retention guarantee and lead to normal destruction;
they do not justify unsafe transfer or undefined behavior.

## Verification gaps

| Capability | Current evidence | Remaining check |
|---|---|---|
| Lifecycle | API 36.1 NativeActivity runtime start/resume/config/destroy, rotation in place, orientation recreation, and background/save/kill/relaunch | Exact per-state transition log against the Essenty state machine on more OS versions |
| StateKeeper | Host codec tests; API 36.1 saved envelope restored after orientation recreation and after real process death (`am kill` + relaunch) | Force-stop correctly discards state and is not a valid test; no further check needed |
| InstanceKeeper core | Implemented with local `Rc` | Core is unchanged |
| NativeActivity config retention | API 36.1 runtime; 50 config events plus rotation spot-checks, same thread/keeper/value, one final drop | Other OS/API combinations and unlisted recreation are outside guarantee |
| Host diagnostic | Host unit-tested; compiles for both Android ABIs | One live-device readout print from a device run |
| Back below 33 | Android target compile | Runtime on older supported API; no runnable pre-33 image in this environment |
| API 33 back | Callback interface selected and compiled | API 33 emulator registration/invocation; image download stalled, no AVD provisioned |
| API 34+ back | API 36 PoC runtime; integrated API 36.1 adapter attach, cancel, committed invoke, teardown, and post-destroy silence verified | Separate API 34 phone run; only the XR headset image is installed |
| Natural gesture progress | Synthetic `BackEvent` mapping verified; shell gestures deliver start/progress/cancel at `progress == 0.0` | Naturally varying progress needs an interactive gesture |
| Adapter stress | 50 config events showed one back attachment and stable lifecycle observer count | 50–100 gesture cycles need a non-exiting stress harness |

The Android SDK was available at `~/Library/Android/sdk`; `adb` was invoked by
its full path. The configured API 36.1 emulator was used. API 33 provisioning
was attempted (`sdkmanager` install of the API 33 Google-APIs ARM image) but
stalled with zero bytes in ten minutes, so no API 33 AVD was created.

## Decompose-rs readiness

A future Decompose-rs layer can be built on the four primitives without
architectural blockers. It receives the `LifecycleRegistry`,
`StateKeeper`/`NativeActivityState` keeper, local `InstanceKeeper`, and
`BackDispatcher` values directly — all thread-confined Rust types with no
Android or JNI types in their signatures. No global runtime singleton exists;
each `android_main` invocation owns its instances. No `Send`/`Sync` is
required globally; the back adapter's internal channel moves plain data to the
owning thread. The known integration work is product-level, not architectural:
drive the lifecycle from `poll_events`, persist through `SaveState`,
synchronize back registration after handler changes, and declare
`configChanges`. No speculative abstraction is added for it.
