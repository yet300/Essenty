# AndroidX integration and APK packaging

## Decision

**A Cargo dependency cannot, by itself, put AndroidX classes in an APK.** Cargo
builds the Rust library; the Android application packager builds the APK and
resolves Maven/AAR dependencies. Cargo build-script output has instructions for
Rust compilation and native linking, but no standard instruction to add an AAR,
run D8, merge Android resources/manifests, or modify an application's Gradle
dependency graph. AndroidX libraries are distributed through Google Maven and
must be included by the application packaging step. See the [Cargo build-script
contract](https://doc.rust-lang.org/cargo/reference/build-scripts.html),
[Android dependency guide](https://developer.android.com/build/dependencies),
and [AAR format guide](https://developer.android.com/studio/projects/android-library).

This is a packaging boundary, not a JNI limitation. JNI can call a class only
after that class has been packaged and loaded. A crate cannot assume that a
host already has AndroidX: a plain `android.app.NativeActivity` APK need not
contain any AndroidX code. Nor can it assume the host uses compatible versions.

The smallest practical integration is an **internal Android library module**
that contains the bridge's concrete JVM classes and declares stable AndroidX
dependencies. An existing Gradle Android application adds that module/AAR once,
and packages the Rust `cdylib` for each ABI as it normally would. The app's
feature code can remain Rust. A Rust-first packaging tool could generate and
own this Gradle project, hiding Gradle from the developer, but a packaging step
is still needed. This repository does not currently ship such a validated
generator or a published AAR. It therefore makes no Cargo-only AndroidX claim.

The repository now includes a **packaging proof** in
[`android/androidx-bridge`](../android/androidx-bridge): an Android library
module declaring these dependencies, and a minimal host app depending on that
module. Its Java availability check references all required API types. It is
not yet a callback shim or a runtime adapter. Copying only its local `.aar`
into an app would lose Maven transitive dependency metadata; consume it as a
Gradle project dependency or publish it with its dependency metadata. A
production artifact must also add concrete callback classes and JNI code.

## Answers to the packaging questions

1. **Classes in the APK:** the Android packager resolves
   `androidx.lifecycle:lifecycle-runtime`,
   `androidx.lifecycle:lifecycle-viewmodel`,
   `androidx.savedstate:savedstate`, and `androidx.activity:activity` from
   Google Maven, along with transitive dependencies, then dexes and packages
   them with the bridge module. A host that already contains these
   artifacts may use them, subject to version compatibility. A raw JAR copy is
   insufficient as a general AAR solution because resources, manifests, and
   transitive dependencies must also be handled.
2. **`android-activity`:** its `NativeActivity` backend remains usable without
   AndroidX. Its stock host is not an AndroidX `LifecycleOwner`,
   `SavedStateRegistryOwner`, `ViewModelStoreOwner`, or
   `OnBackPressedDispatcherOwner`. Merely adding dependencies cannot make it
   one. An AndroidX-capable `NativeActivity` subclass or a separate host is
   needed, and its main-thread callbacks must be reconciled with the
   `android-activity` Rust event-loop thread. This has not been runtime tested.
3. **Existing Android project:** the packaging path works when its host build
   includes the bridge module plus the Rust `.so`. The callback bridge remains
   unimplemented, so full attachment to those owners is not yet available.
   No application-authored Kotlin/Java callback forwarding is conceptually
   required; the private bridge should own that forwarding.
4. **Gradle:** required by the recommended existing-project path to resolve
   Maven/AAR artifacts and perform Android packaging. In principle a different
   packager could implement the same dependency, D8, resource, and manifest
   work. This repository has not verified such a toolchain.
5. **Automation:** a Rust-first Android packager can generate a Gradle project,
   dependencies, and bridge module, then invoke Gradle. A Cargo `build.rs`
   cannot safely alter an arbitrary host APK or Gradle project.
6. **Consumer configuration:** select a compatible AndroidX host; add the
   bridge Android library project dependency and package the Rust `.so`. The host must
   arrange a Rust entry point, as for any Rust Android library. All routine
   lifecycle/state/back callback implementations should reside in the bridge.
7. **`cargo add essenty` sufficient?** No. It adds Rust code, not an APK or
   AndroidX Maven artifacts. It remains sufficient for the platform-free core.
8. **Smallest unavoidable step:** configure the APK packager to include the
   AndroidX bridge AAR/module and Rust native library. A future Rust-first
   packager can automate this step for applications it owns.
9. **Direct JNI versus JVM subclass:** JNI can read lifecycle state, call
   registry methods, consume a restored Bundle, get a ViewModelStore, and set
   callback enablement. `DefaultLifecycleObserver` and
   `SavedStateRegistry.SavedStateProvider` are interfaces: a dynamic proxy
   could implement them, though a tiny concrete shim is simpler to test.
   `ViewModel` (for `onCleared`) and abstract `OnBackPressedCallback` require
   concrete JVM subclasses or generated bytecode; a dynamic proxy cannot
   extend an abstract class. The bridge should contain only event translation
   and retention hooks; Rust core types remain authoritative.
10. **Maintenance:** pin and test a stable AndroidX baseline, let Gradle
    resolve its transitive graph, and test both the baseline and newer host
    versions. Any Java/Kotlin binary or JNI signature change requires bridge
    review. Never silently assume the host's version is compatible.

## Version baseline

For a first bridge, use the stable baseline **Activity 1.8.2**, **Lifecycle
2.7.0**, and **SavedState 1.2.1**, letting Gradle reconcile transitive
dependencies. Activity's `OnBackPressedCallback.handleOnBackStarted` and
`handleOnBackProgressed` were [added in
1.8.0](https://developer.android.com/reference/androidx/activity/OnBackPressedCallback);
1.8.2 is a [stable patch release](https://developer.android.com/jetpack/androidx/releases/activity#1.8.2).
Lifecycle 2.7.0 and SavedState 1.2.1 are
[stable releases](https://developer.android.com/jetpack/androidx/releases/lifecycle)
and [documented releases](https://developer.android.com/jetpack/androidx/releases/savedstate),
respectively. The API floors for the planned implementation are
`DefaultLifecycleObserver` in [Lifecycle
2.4.0](https://developer.android.com/reference/androidx/lifecycle/DefaultLifecycleObserver),
`ViewModelStore` in [Lifecycle
2.0.0](https://developer.android.com/reference/androidx/lifecycle/ViewModelStore),
SavedStateRegistry consume/register in [SavedState
1.0.0](https://developer.android.com/reference/androidx/savedstate/SavedStateRegistry),
and predictive back callbacks in [Activity
1.8.0](https://developer.android.com/jetpack/androidx/releases/activity#1.8.0).
The chosen versions are a conservative compatible baseline rather than these
individual API floors. Confirm the complete dependency graph and minSdk in a
built host before publishing an AAR. Avoid a floating `+` or alpha version.

## Upstream behavior to preserve

The reference is upstream's current [lifecycle adapter](https://github.com/arkivanov/Essenty/blob/master/lifecycle/src/androidMain/kotlin/com/arkivanov/essenty/lifecycle/AndroidExt.kt),
[state adapter](https://github.com/arkivanov/Essenty/blob/master/state-keeper/src/androidMain/kotlin/com/arkivanov/essenty/statekeeper/AndroidExt.kt),
[Bundle serialization adapter](https://github.com/arkivanov/Essenty/blob/master/state-keeper/src/androidMain/kotlin/com/arkivanov/essenty/statekeeper/BundleExt.kt),
[instance adapter](https://github.com/arkivanov/Essenty/blob/master/instance-keeper/src/androidMain/kotlin/com/arkivanov/essenty/instancekeeper/AndroidExt.kt),
and [back adapter](https://github.com/arkivanov/Essenty/blob/master/back-handler/src/androidMain/kotlin/com/arkivanov/essenty/backhandler/AndroidBackHandler.kt).

* Lifecycle maps all five states. A single observer translates create, start,
  resume, pause, stop, and destroy; duplicate subscription is rejected and the
  observer is removed at teardown. The Rust registry supplies replay and
  state tracking. AndroidX `addObserver` may synchronously deliver catch-up
  events, so initialization must avoid double replay.
* State consumes a registry entry under `STATE_KEEPER_STATE` by default or a
  custom key, and registers one provider at that key. The provider may emit an
  empty Bundle when saving is disabled. Opaque Rust bytes should be stored in
  Bundle byte arrays under a private versioned schema; the core's unconsumed
  values and codec policy stay in Rust. Duplicate provider keys must be
  surfaced as errors, not replaced silently: SavedState 1.2.1's
  `registerSavedStateProvider` uses `putIfAbsent` and throws on conflict
  (confirmed in its published source JAR). Upstream's `BundleExt.kt` uses a
  `Parcelable` holder with lazy `kotlinx.serialization` bytes and temporarily
  switches the Bundle class loader when reading. A Rust byte-array schema
  preserves state behavior but does not read upstream Kotlin's Bundle payloads;
  cross-language migration would need an explicit codec.
* Instance retention uses one `ViewModel` per store to own one Rust dispatcher
  handle. Configuration recreation reuses that ViewModel; final `onCleared`
  destroys the dispatcher. Discard mode destroys it immediately and creates a
  fresh dispatcher. A global Rust map is not a substitute for the store.
* Back uses one `OnBackPressedCallback` for one Rust `BackDispatcher`, with
  enabled state synchronized after all Rust registrations/state changes.
  Registration may be lifecycle aware or unconditional. Translate ordinary
  back and predictive start/progress/cancel/commit, including swipe edge,
  progress, and touch coordinates. API 34+ supplies system predictive
  progress through AndroidX; lower API levels still need ordinary back.

## Boundary and lifetime design

The eventual public entry points should accept opaque, Android-only safe
owner/dispatcher handles acquired by a Rust host adapter. They must not put
`JNIEnv`, `jobject`, `Bundle`, or JVM class names into core crates. The
AndroidX bridge must run on the Android main thread: lifecycle, saved-state,
ViewModelStore, and back callbacks are main-thread-oriented. The Rust core
uses `Rc`/`RefCell` and is not generally `Send`, so callbacks may access it
only on its owning thread. JNI should retain a global reference only for the
duration of an attachment, never a local reference. A native callback handle
must be invalidated before releasing Rust state, with late callbacks becoming
no-ops. Rust panics must be caught before crossing JNI; Java exceptions must
be checked and translated into Rust errors. Destruction and explicit detach
must remove observers/providers/callbacks as applicable.

This is a design boundary, **not an implementation claim**. No functional
AndroidX callback bridge or JNI safety model is runtime verified in the
current tree. The current NativeActivity backend and its host-side mapping helpers are
Rust-specific Android extensions; they do not satisfy upstream AndroidX
parity. See [Android status](ANDROID.md).

## Packaging verification (2026-09-24)

With Android SDK 35 and Gradle 9.6.1/Android Gradle Plugin 9.3.1, both
`:bridge:assembleDebug` and `:packaging-test-app:assembleDebug` completed.
The test APK contains dex **class definitions** for
`androidx.activity.OnBackPressedCallback`,
`androidx.lifecycle.ViewModelStore`, and
`androidx.savedstate.SavedStateRegistry`, confirmed with Android SDK
`dexdump`. This proves a Gradle project dependency carries the runtime classes
into an APK. It does **not** prove a local AAR file alone carries its Maven
dependencies, nor does it prove any AndroidX callback reaches Rust. The test
app has no Activity and was not run on a device/emulator.

To reproduce, set `ANDROID_HOME` to an installed SDK, make Gradle available
as `gradle` or set `GRADLE_BIN`, and run:

```sh
android/androidx-bridge/verify-packaging.sh
```

The script builds both modules and checks required class definitions in the
APK dex files with SDK `dexdump`.

For an existing Gradle host, the equivalent **packaging-only** setup is to
include this `bridge` module as a project in `settings.gradle.kts` and depend
on it from the app module:

```kotlin
include(":essentyAndroidXBridge")
project(":essentyAndroidXBridge").projectDir =
    file("<path-to-essenty>/android/androidx-bridge/bridge")
```

```kotlin
dependencies {
    implementation(project(":essentyAndroidXBridge"))
}
```

The host must also package its Rust `.so` for each supported ABI, as it does
for any Rust Android library. The included module uses AGP 9.3.1 for the
reproducible proof build; a host that imports just the subproject uses its own
compatible Android Gradle Plugin. These snippets make the dependencies
available in the APK but do not yet activate an AndroidX Rust adapter.
