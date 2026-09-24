# Direct platform predictive back with a Rust-owned dynamic proxy

Decision: RUST DYNAMIC PROXY BACKEND VIABLE

**Scope and confidence.** This is an Android 36 emulator proof for the API 34+ path, with high confidence that `jni-min-helper` 0.4.7 implements `OnBackAnimationCallback` and forwards its four methods to Rust. Numeric field fidelity is proven by runtime-created `android.window.BackEvent` objects. An `invoke-only` build also proved that the proxy directly implements `OnBackInvokedCallback` on the emulator; API 33 itself and varied progress values from a physical gesture have not been runtime verified. This is a proof of concept, not a production `essenty-android` backend.

## What was built

The independent [example](../examples/android-predictive-proxy-poc/) is a Rust `cdylib` packaged with `cargo-apk2`. Its manifest declares `android.app.NativeActivity`, `android:enableOnBackInvokedCallback="true"`, and no consumer Java, Kotlin, AndroidX, or Gradle project. `cargo-apk2` built and signed an APK containing the Rust library and its normal NativeActivity `classes.dex`. `jni-min-helper` privately embeds an `InvocHdl` invocation-handler DEX in the Rust library and loads it with `InMemoryDexClassLoader`. The generated Java `Proxy` implements `android.window.OnBackAnimationCallback` on API 34+, which also extends `OnBackInvokedCallback`; API 33 selects `OnBackInvokedCallback` directly. Below API 33 the example uses the native key route.

This matters because `java.lang.reflect.Proxy` alone still needs a JVM `InvocationHandler`. `jni-min-helper` supplies one internally; no consumer JVM work is required. The private DEX is an implementation detail of the crate, not a consumer source file.

The example calls `Activity.getOnBackInvokedDispatcher()`, `registerOnBackInvokedCallback(0, proxy)`, and `unregisterOnBackInvokedCallback(proxy)` on the Java main thread through `AndroidApp::run_on_java_main_thread`. The Rust callback reads all four [BackEvent](https://developer.android.com/reference/android/window/BackEvent) getters (`getProgress`, `getSwipeEdge`, `getTouchX`, `getTouchY`) and sends a Rust enum through a channel to the NativeActivity event-loop thread, where it calls the existing `BackDispatcher`. The initial progress is sent as an immediate `Progressed` event following `Started` because the current core `Started` shape has no progress field; no value is silently dropped. `onBackCancelled` and `onBackInvoked` have no `BackEvent` argument.

## Runtime evidence

Device: `Resizable_Experimental`, arm64 emulator, SDK 36, 1080 × 2400. The APK built, installed, launched, and registered proxy 42. `adb shell input swipe` then produced the following real platform callbacks on the Java main thread (PID 4880, TID 4880) and corresponding Essenty events on the Rust thread (TID 4895):

| Input | Java → Rust callback sequence | Field evidence |
| --- | --- | --- |
| Edge swipe from x=1 to x=900, 1000 ms | `Started`, 3 × `Progressed`, `Invoked` | edge `0` mapped to `Left`; x `18.081055`, y `1200.0` |
| Short edge swipe from x=1 to x=35, 500 ms | `Started`, 5 × `Progressed`, `Cancelled` | edge `0` mapped to `Left`; x `16.299805`, y `1300.0` |
| Right edge swipe in earlier run | `Started`, 3 × `Progressed`, `Invoked` | edge `1` mapped to `Right`; x `1062.6885`, y `900.0` |

The first two gestures made **5** and **7** Java → Rust transitions respectively, one per platform callback. First progress callbacks were near the start; observed later spacing was irregular (roughly 5–250 ms in the logged gestures). These shell-injected gestures are not a representative frame-rate benchmark. The `BackEvent.getProgress()` values supplied by this emulator during those swipes were **all 0.0**, and x/y stayed at the start point. Thus live variable progress and moving touch coordinates are **not proven** by the shell gestures. This is an emulator/input limitation or platform behavior not yet isolated; it must be checked with an interactive Android 14+ gesture before calling the backend production ready.

To distinguish that limitation from a broken JNI mapping, pressing `S` in the example constructs actual Android `BackEvent` Java objects and invokes methods on the Java proxy. This runtime test produced `Progressed` values `0.15`, `0.67`, `0.92`, touch positions `(75,410)`, `(350,420)`, `(700,430)`, and an edge change `0 → 1`; the Rust `BackDispatcher` logged the same values and `Left → Right`. These events prove numeric extraction and forwarding through the proxy, but they are **synthetic calls**, not a real system gesture. The `Started` value `0.0` likewise reached the dispatcher as its first `Progressed` event.

Pressing Space requested explicit unregister. The Java main thread logged `UNREGISTERED proxy=42` before `PROXY_DROPPED`; a later Back input emitted no proxy callback and no `AndroidRuntime` crash. Calling unregister twice becomes a no-op through the registration `Option`. The proof retains the dispatcher `GlobalRef`, proxy `GlobalRef`, and Rust closure together until successful unregister. On a JNI unregister error it retains the registration for retry rather than dropping the callback under the Java dispatcher. On `MainEvent::Destroy` it posts an unregister request before the Rust event loop exits; the posted closure retains the registration until it runs. Closure delivery after the Rust receiver closes fails harmlessly instead of touching a destroyed `BackDispatcher`.

Recreating the Activity while proxy 107551561510 was still registered logged `DESTROY_UNREGISTER_REQUESTED` and `DESTROY` on the Rust thread, then `UNREGISTERED proxy=107551561510` and `PROXY_DROPPED` on the Java main thread. The next Activity had a distinct proxy. A separate APK built with `--features invoke-only` registered `OnBackInvokedCallback` directly; both a real edge swipe and an ordinary `KEYCODE_BACK` input logged `JNI_CALLBACK Invoked` and the Rust `BackDispatcher` logged `Invoked`. This validates the API 33 interface type, though the emulator itself was API 36.

The proof did **not** measure global-reference counts under repeated Activity recreation, verify an unregister failure path on device, or measure reflection overhead under a sustained 60/120 Hz gesture. Those are production integration gates. No evidence here calls for a private `OnBackAnimationCallback` class: the dynamic proxy both registers and dispatches in a real Android process. A private DEX shim could be considered if future profiling shows reflection too costly, and `cargo-apk2`'s observed Java compilation stage shows packaging without consumer Gradle is feasible. It is not required by this proof.

## Reproduce

From `examples/android-predictive-proxy-poc`, with Android SDK/NDK configured:

```sh
cargo apk2 build
adb install -r target/debug/apk/essenty_predictive_proxy_poc.apk
adb shell am start -n dev.essenty.predictiveproxy/android.app.NativeActivity
adb shell input swipe 1 1200 900 1200 1000
adb shell input swipe 1 1300 35 1300 500
adb shell input keyevent KEYCODE_S
adb shell input keyevent KEYCODE_SPACE
adb logcat -d -s EssentyProxyPOC:I AndroidRuntime:E
```

The example uses `S` for the numeric-field probe and Space for teardown. Use a live edge gesture to test platform-supplied variable progress.
Build with `cargo apk2 build --features invoke-only` to exercise the API 33 interface on an API 34+ device.

## AndroidX removal scope for the next task

The AndroidX experiment is commit `6da0a0f` (`build(android): verify AndroidX packaging boundary`). Its self-contained packaging files are the **entire** `android/androidx-bridge/` directory, including the Gradle builds, Java availability probe, test app, and `verify-packaging.sh`. `docs/ANDROIDX_INTEGRATION.md` documents that experiment. Commit `6da0a0f` also changed `README.md`, `docs/ANDROID.md`, `docs/SEMANTIC_COMPATIBILITY.md`, and `docs/TARGETS.md`; only the AndroidX-specific paragraphs in those files should be edited after production integration. Do **not** revert commit `efcbb1f`, which added the Rust swipe-edge and touch-position data needed here. No AndroidX files or branch were removed in this proof.

Platform/API references: [OnBackInvokedDispatcher](https://developer.android.com/reference/android/window/OnBackInvokedDispatcher), [OnBackAnimationCallback](https://developer.android.com/reference/android/window/OnBackAnimationCallback), [BackEvent](https://developer.android.com/reference/android/window/BackEvent), [predictive-back opt-in](https://developer.android.com/guide/navigation/custom-back/predictive-back-gesture), [android-activity](https://docs.rs/android-activity/0.6.1/android_activity/), and [jni-min-helper DynamicProxy](https://docs.rs/jni-min-helper/0.4.7/jni_min_helper/struct.DynamicProxy.html).
