# Essenty Rust

Essenty for Rust is a Rust-native port of the Essenty architectural primitives,
based on the behavior of [Ark Ivanov's Essenty](https://github.com/arkivanov/Essenty).
Application developers consume it entirely from Rust. Kotlin, Java, Swift,
Objective-C, and C/C++ wrappers are not required. Platform FFI exists only
inside platform adapters where operating system APIs require it.

> **Status: early development (`0.1.0`).** The four core primitives are tested.
> macOS, UIKit, Web, and Rust NativeActivity lifecycle adapters now observe
> real platform events, with the verification levels and gaps documented in
> [Android](docs/ANDROID.md), [Apple](docs/APPLE.md), and [Web](docs/WEB.md).
> AndroidX saved state, retained objects, and predictive back are still planned.

This project is **not** affiliated with the Essenty authors. No Essenty source
code was copied; only public behavioral concepts (lifecycle states, saved
state, retained instances, back dispatch) informed an original Rust-native
design.

## Architecture

```text
essenty-lifecycle
essenty-state-keeper
essenty-instance-keeper
essenty-back-handler
          ↑
          │
 ┌────────┼─────────┐
 │        │         │
Android  Apple      Web
```

- **Core crates are platform-free.** They depend only on `serde` (state
  keeper), `thiserror` (typed errors), and `std` containers used in an
  `alloc`-compatible way. They must never depend on platform crates.
- **Platform crates are adapters.** They wrap the core for Android, the Apple
  family, and Web/WASM. Platform SDK/FFI dependencies stay
  behind target-gated dependencies inside the adapter crate.
- **Capability-oriented, not triple-oriented.** There is one crate per
  platform family — never one crate per CPU architecture. OS differences
  inside the Apple family are modules, not crates.
- **No executor coupling.** The core is synchronous and runtime-agnostic.
  There is intentionally **no Tokio** (or any async runtime) dependency; async
  will only appear where a concrete lifecycle integration genuinely needs it,
  via runtime-agnostic `Future`s.

## Workspace crates

| Crate | Role | State |
|---|---|---|
| `essenty` | Umbrella facade re-exporting the core APIs (`essenty = "0.1"`) | Implemented |
| `essenty-lifecycle` | Ordered states (`Initialized/Created/Started/Resumed/Destroyed`), validated transitions, RAII subscriptions, manually controlled registry | Implemented + tested |
| `essenty-state-keeper` | Byte-oriented providers, single-shot restore consumption, deterministic ordered save, pluggable `serde` codecs (no hard-coded JSON) | Implemented + tested |
| `essenty-instance-keeper` | `Rc`-shared retained objects with deterministic `Drop` cleanup, per-key type checking | Implemented + tested |
| `essenty-back-handler` | Priority-ordered dispatch, enable/disable, regular + predictive (`start/progress/cancel/invoke`) gesture model with gesture claiming | Implemented + tested |
| `essenty-android` | Rust NativeActivity lifecycle, ordinary Back key and saved-state byte bridge; host-testable mappings | NativeActivity compile-tested; AndroidX and predictive back planned |
| `essenty-apple` | Application notifications through `objc2` on macOS, iOS, tvOS, visionOS, Catalyst, and watchOS | Native observers compile-tested; device tests planned |
| `essenty-web` | Automatic visibility and page transition observation; opt-in `popstate` listener and browser storage persistence | WASM compile-tested; browser tests planned |

The umbrella `essenty` crate optionally provides `Runtime`, `PlatformEvent`, and
`DispatchResult` behind the `runtime` feature. None of the four primitives or
platform adapters requires it. See
[`docs/FFI_ARCHITECTURE.md`](docs/FFI_ARCHITECTURE.md) for ownership and FFI
rules, and [`docs/SEMANTIC_COMPATIBILITY.md`](docs/SEMANTIC_COMPATIBILITY.md)
for the audited upstream behavior and intentional Rust differences.

## Platforms

The core remains framework neutral on Linux and Windows. Platform adapters
are selected at compile time. See [the exact 21-target matrix](docs/TARGETS.md)
for target-by-target verification. `cargo check` does not establish runtime
behavior on an emulator, simulator, device, or browser.

## Quick start

```toml
[dependencies]
essenty = "0.1"
```

```rust
use essenty::{BackDispatcher, InstanceKeeper, LifecycleRegistry, StateKeeper};

// Lifecycle: validated transitions + RAII observers.
let lifecycle = LifecycleRegistry::new();
let _guard = lifecycle.subscribe(|state| println!("now {state:?}"));
lifecycle.create().unwrap();
lifecycle.start().unwrap();
lifecycle.resume().unwrap();

// State: register providers, save deterministically, restore single-shot.
let mut keeper = StateKeeper::new();
keeper.register("counter", || 41_u32.to_le_bytes().to_vec()).unwrap();
let snapshot = keeper.save().unwrap();
let mut restored = StateKeeper::with_restored(snapshot);
let bytes = restored.consume_bytes("counter").unwrap();
assert_eq!(u32::from_le_bytes(bytes.try_into().unwrap()), 41);

// Retention: same key + type yields the same Rc; Drop cleans up.
let mut instances = InstanceKeeper::new();
let model = instances.get_or_create("model", || vec![1, 2, 3]).unwrap();
assert_eq!(*model, vec![1, 2, 3]);

// Back: priority dispatch with predictive gesture support.
let mut back = BackDispatcher::new();
back.register(0, true, |event| println!("back {event:?}"));
assert!(back.back());
```

For an application that wants one optional event entry point, enable
`essenty = { version = "0.1", features = ["runtime"] }`:

```rust
use essenty::{PlatformEvent, Runtime, LifecycleState};

let mut runtime = Runtime::new();
runtime.dispatch(PlatformEvent::Lifecycle(LifecycleState::Resumed)).unwrap();
let result = runtime.dispatch(PlatformEvent::BackPressed).unwrap();
assert_eq!(result.back_handled, Some(false));
```

`serde`-based state with a caller-chosen codec (JSON shown as one option;
`postcard`/`bincode` work the same way):

```rust
use essenty::StateKeeper;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Counter { value: u32 }

let mut keeper = StateKeeper::new();
keeper.register_value(
    "counter",
    || Counter { value: 7 },
    |v| serde_json::to_vec(v).map_err(|e| e.to_string()),
).unwrap();
let snapshot = keeper.save().unwrap();

let mut restored = StateKeeper::with_restored(snapshot);
let value: Option<Counter> = restored
    .consume_value("counter", |b| serde_json::from_slice(b).map_err(|e: serde_json::Error| e.to_string()))
    .unwrap();
assert_eq!(value, Some(Counter { value: 7 }));
```

See [`examples/README.md`](examples/README.md) and
`crates/essenty/examples/counter.rs` for a runnable end-to-end demo.

On Web, a Rust application can attach lifecycle observation directly:

```rust,ignore
use essenty_web::wasm::BrowserLifecycle;

let lifecycle = BrowserLifecycle::new()?;
let _subscription = lifecycle.registry().subscribe(|state| {
    // Handle the new Essenty lifecycle state in Rust.
});
```

See the platform guides for the supported Rust host models and their limits.

## Build / test / lint

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Cross-target spot checks (linking Apple/Windows targets may need an SDK;
`cargo check` is the meaningful gate in CI):

```bash
rustup target add wasm32-unknown-unknown
cargo check -p essenty-web --target wasm32-unknown-unknown
cargo check --workspace --target aarch64-apple-ios
cargo check --workspace --target x86_64-unknown-linux-gnu
```

## Design notes

- **Rust ownership over Kotlin interfaces.** Observers use RAII guards,
  retained objects use `Rc` + `Drop`, back callbacks use explicit id tokens —
  each chosen so misuse is a compile-time or explicit-runtime outcome, not a
  lifecycle leak.
- **Single-threaded cores.** Core types are `!Send`/`!Sync` (via `Rc`,
  `RefCell`, `FnMut`) where their ownership requires it. Multithreaded hosts
  confine the runtime to one thread or provide a dedicated wrapper;
  `Send`/`Sync` is never imposed without justification.
- **Localized unsafe.** Pure core crates forbid unsafe code. `objc2`
  notification registration is confined to Apple adapter modules with
  documented safety assumptions.
- **No desktop framework forced.** Core compiles for Windows/Linux today;
  `winit`/`Tauri`/`Slint` adapters can arrive later as separate crates.

## License

Apache-2.0. See [LICENSE](LICENSE).
