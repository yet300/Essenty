# Essenty Rust

A cross-platform application lifecycle/state runtime for Rust, inspired by
[Ark Ivanov's Essenty](https://github.com/arkivanov/Essenty) but designed
idiomatically for Rust from the ground up.

> **Status: early development (`0.1.0` bootstrap).** The pure Rust core
> (lifecycle, state keeper, instance keeper, back handler) is implemented,
> tested, and covered by CI. Platform adapters (Android, Apple, Web) exist as
> documented bootstrap mappings; full FFI/event wiring (JNI, `objc2`
> observers, browser listeners) is planned follow-up work. A Decompose-like
> architecture layer will be built on top of this foundation.

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
  family, and Web/WASM. Platform SDK/FFI dependencies (when added) stay
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
| `essenty-android` | `Activity` lifecycle, `SavedStateRegistry`, `OnBackPressedDispatcher`/Predictive Back mappings | Bootstrap adapters + tests; JNI planned |
| `essenty-apple` | One Apple-family crate (iOS, macOS, watchOS, tvOS, visionOS, Mac Catalyst) with shared implementation | Bootstrap adapters + tests; `objc2` observers planned |
| `essenty-web` | Visibility → lifecycle, `popstate` → back, storage key namespacing; `wasm32`-only live bindings seam | Bootstrap mappings + tests; listener wiring planned |

## Platforms

Core crates are written against `core`/`alloc`-compatible containers and
avoid filesystem, networking, and global runtimes, so they build anywhere
Rust builds. Target triples differ by CPU, but architecture is
capability-oriented: no crate exists merely because a triple differs.

| Family | Triples (architectural scope) | This milestone |
|---|---|---|
| Android | `aarch64-linux-android`, `x86_64-linux-android` | Adapter compiles; `cargo check` in CI with NDK; JNI planned |
| iOS | `aarch64-apple-ios`, `aarch64-apple-ios-sim` | `cargo check` on macOS CI; `objc2` wiring planned |
| macOS | `aarch64-apple-darwin`, `x86_64-apple-darwin` | Host-tested on Apple Silicon; x86_64 via `cargo check` |
| Windows | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | `cargo check`/test in CI; no GUI framework forced (future `essenty-winit`, `essenty-tauri`, … crates) |
| Linux GNU | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | Host-tested + cross-check in CI |
| Linux musl | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` | `cargo check` in CI where toolchains allow |
| Web | `wasm32-unknown-unknown` | `cargo check` in CI; runtime browser wiring planned |
| watchOS / tvOS / visionOS / Catalyst | `aarch64-apple-watchos[-sim]`, `aarch64-apple-tvos[-sim]`, `aarch64-apple-visionos[-sim]`, `*-ios-macabi` | `cargo check` on macOS CI where SDKs allow; shared Apple implementation |

Labels used below and in CI: **implemented** (real behavior + tests),
**compile-tested** (`cargo check` only, linking/runtime may need an SDK),
**planned** (documented seam, no code yet).

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
  `RefCell`, `FnMut`) so they work on WASM and single-threaded UI loops.
  Multithreaded hosts confine the object to one thread or wrap it in a
  `Mutex` — `Send`/`Sync` is never imposed without justification.
- **Typed errors, no panics.** Library code returns `LifecycleError`,
  `StateKeeperError`, `InstanceKeeperError`, `BackError`; `unwrap`/`expect`
  appear only in tests. No `unsafe` in this milestone
  (`unsafe_code = "forbid"` for pure crates).
- **No desktop framework forced.** Core compiles for Windows/Linux today;
  `winit`/`Tauri`/`Slint` adapters can arrive later as separate crates.

## License

Apache-2.0. See [LICENSE](LICENSE).
