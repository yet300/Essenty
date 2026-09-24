# Native Android backend decision and verification

## Decision

The primary Android backend is Rust `NativeActivity` plus direct platform APIs.
The consuming application remains Rust-only. Internal JVM machinery packaged
by `jni-min-helper` is acceptable; AndroidX and consumer-owned JVM callback
code are not required.

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

The production adapter has since moved into `crates/essenty-android`. It
compile-checks for both Android ABIs. This does not replace the PoC runtime
evidence: API 33 and integrated-crate emulator runs remain unverified.

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
checks, but no emulator recreation or process-relaunch evidence.

The adapter intentionally reports **Android Essenty semantic behavior**. It
does not claim implementation identity with upstream's AndroidX owners. The
NativeActivity lifecycle observes the NativeActivity event loop; state uses
native saved bytes; and predictive back uses direct `android.window` APIs.

## Retained-instance blocker

The core `InstanceKeeper` stores non-`Send` `Rc` values. NativeActivity
recreation does not provide a demonstrated thread-stable handoff in this
repository, and a saved logical token cannot distinguish a delayed replacement
from a failed recreation or stale saved state. A process-local registry would
therefore either share `Rc` unsafely, assume undocumented thread identity, or
retain entries without a reliable deterministic cleanup event. No AndroidX
`ViewModelStore` substitute is added. Instance retention remains an explicit
unimplemented capability until ownership transfer and stale-entry cleanup can
be proven.

## Verification gaps

| Capability | Current evidence | Remaining check |
|---|---|---|
| Lifecycle | Android target compile | API 36 NativeActivity runtime recreation |
| StateKeeper | Host codec tests; Android target compile | Emulator recreation and process relaunch |
| InstanceKeeper | Feasibility analysis only | Safe cross-recreation identity/cleanup design |
| Back below 33 | Android target compile | Runtime on older supported API |
| API 33 back | Callback interface selected and compiled | API 33 emulator registration/invocation |
| API 34+ back | API 36 PoC runtime; production adapter compile | Integrated adapter runtime on API 34+ |
| Natural gesture progress | Synthetic BackEvent mapping verified | Real interactive gesture with varying progress |
| Lifetime stress | One PoC recreation | 50–100 recreation leak/duplicate-callback test |

The current environment had Android Rust targets installed but no `adb` or
emulator executable on `PATH`; therefore runtime rows remain accurately
unverified.
