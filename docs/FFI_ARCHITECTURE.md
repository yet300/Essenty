# Platform boundary architecture

Essenty has four independent Rust primitives: `LifecycleRegistry`,
`StateKeeper`, `InstanceKeeper`, and `BackDispatcher`. Their core crates have no
dependency on Android, Apple, or browser crates. Applications may use any one
primitive without constructing an umbrella runtime.

Platform crates translate operating system events into these primitives. This
translation is selected by Cargo target configuration, not a runtime platform
enum. The optional `essenty/runtime` feature retains the earlier `Runtime`,
`PlatformEvent`, and `DispatchResult` as a convenience for applications that
want a single event entry point. It is neither the base architecture nor an FFI
ABI. No platform adapter depends on it.

## Ownership and threading

Lifecycle subscriptions use RAII guards. `InstanceKeeper` values use `Rc`
ownership and final `Drop`; logical lifecycle destruction is a separate event.
The core lifecycle and back dispatcher are local to their owning thread. The
macOS and UIKit observers use main-thread Objective-C objects. The Web adapter
owns browser closures and removes listeners on drop. The Android
`NativeActivityLifecycle` wrapper runs in the `android_main` event loop.

## Native boundaries

- Android NativeActivity uses `android-activity`'s Rust API, whose host glue
  bridges Android framework events. The Android-only back adapter uses JNI
  and `jni-min-helper` internally for platform callback registration; no
  consumer-facing Java API is required.
- Apple application lifecycle observers are implemented in Rust with `objc2`,
  `objc2-foundation`, and UIKit or AppKit bindings. Unsafe calls are limited to
  observer registration, initialization, and removal. Selectors correspond to
  methods on a retained observer and callbacks stay on the main thread.
- Web lifecycle observation uses `wasm-bindgen` closures and `web-sys` DOM
  listeners. Browser and WASM types stay in `essenty-web`.

StateKeeper's core values remain opaque bytes. A platform adapter must choose
an outer persistence encoding without imposing JSON, Bundle, or property lists
on the core. NativeActivity persists that envelope through its native saved
state callbacks. Retained-instance handoff remains unimplemented until
thread ownership and stale-recreation cleanup can be proved.

## Back navigation

The core accepts ordinary and predictive back events, including progress.
`NativeActivityLifecycle` routes ordinary key back below API 33. The Android
adapter uses `OnBackInvokedCallback` on API 33 and
`OnBackAnimationCallback` on API 34+ through a private dynamic proxy. On Web,
`popstate` arrives after history navigation; a callback result cannot cancel
that history change. The Web bridge is explicit and does not install a global
navigation interceptor.

## Future Decompose port

Component trees and navigation belong in a future `decompose-rs` crate that
depends on Essenty. Essenty must not depend on that layer.
