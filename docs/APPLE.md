# Apple integration

## Implemented

On macOS, `ApplicationLifecycle::new()` observes `NSApplication` lifecycle
notifications with `objc2-app-kit`. On iOS, tvOS, visionOS, and Mac Catalyst,
the same Rust API observes process-wide `UIApplication` notifications with
`objc2-ui-kit`. The public API exposes only a Rust `LifecycleRegistry`.

```toml
[dependencies]
essenty-apple = "0.1"
```

```rust,ignore
use essenty_apple::ApplicationLifecycle;

let lifecycle = ApplicationLifecycle::new()?; // call on the main thread
let _subscription = lifecycle.registry().subscribe(|state| {
    // Handle state in Rust.
});
```

The observer is removed on drop. Keep the returned value alive for the scope
being observed. Apple UI notifications and callbacks are main-thread bound;
the registry is deliberately not `Send` or `Sync`.

The UIKit adapter observes **application** state. It does not distinguish
individual `UIScene` instances. watchOS currently has only the shared manual
`AppleLifecycle` mapping helper; no WatchKit notification binding is claimed.
Core state, instance, and back primitives still work on every Apple target,
without invented platform adapters.

## Verified

- Shared mapping tests: macOS host tests.
- macOS observer delivery and drop cleanup: ran the
  `macos_notifications` example with synchronous AppKit notifications on a
  macOS host. This does not exercise a full application event loop.
- Native observer code: `cargo check` on macOS, iOS, tvOS, and visionOS targets.
- UIKit notification delivery in a running application: not tested.
- Simulator and device runtime: not tested.

## Planned

- Runtime tests for notification registration, delivery, and drop cleanup.
- Scene-scoped lifecycle if a Rust host needs scene ownership.
- A watchOS native lifecycle observer where WatchKit behavior can be mapped
  without inventing application semantics.

No Swift or Objective-C source is required from the application. Unsafe
Objective-C calls are confined to the adapter and annotated with their
selector and main-thread invariants.
