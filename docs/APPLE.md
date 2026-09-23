# Apple integration

## Implemented

On macOS, `ApplicationLifecycle::new()` observes `NSApplication` lifecycle
notifications with `objc2-app-kit`. On iOS, tvOS, visionOS, and Mac Catalyst,
the same Rust API observes process-wide `UIApplication` notifications with
`objc2-ui-kit`. On watchOS 7 and later, it observes `WKApplication`
notifications from WatchKit and reads the current `WKApplicationState` at
attachment. The public API exposes only a Rust `LifecycleRegistry`.
The watchOS mapping follows Apple's documented
[application notifications](https://developer.apple.com/documentation/watchkit/wkapplication)
and [application state](https://developer.apple.com/documentation/watchkit/wkapplication/applicationstate).

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

The UIKit and WatchKit adapters observe **application** state. They do not
distinguish individual scenes. The watchOS observer maps active to Resumed,
inactive to Started, and background to Created. Its registry is destroyed
when the observer is dropped; WatchKit has no matching termination
notification in this adapter. Core state, instance, and back primitives work
on every Apple target.

## Verified

- Shared mapping tests: macOS host tests.
- macOS observer delivery and drop cleanup: ran the
  `macos_notifications` example with synchronous AppKit notifications on a
  macOS host. This does not exercise a full application event loop.
- Native observer code: `cargo check` on macOS, iOS, tvOS, visionOS, and
  watchOS targets.
- UIKit notification delivery in a running application: not tested.
- Simulator and device runtime: not tested.

## Planned

- Runtime tests for notification registration, delivery, and drop cleanup.
- Scene-scoped lifecycle if a Rust host needs scene ownership.
- watchOS simulator or device verification of notification delivery and
  observer removal.

No Swift or Objective-C source is required from the application. Unsafe
Objective-C calls are confined to the adapter and annotated with their
selector and main-thread invariants.
