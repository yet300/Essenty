# Public API audit

Scope: every public item in all 8 workspace crates, reviewed 2026-09-24
(post-fix). Guiding questions: necessary? idiomatic? ownership clear? error
behavior clear? platform types leaking? Decompose-rs dependable without
near-term breakage? Cosmetic-only churn was avoided; the four breaking-adjacent
changes made are recorded in
[`UPSTREAM_PARITY_AUDIT.md §10`](UPSTREAM_PARITY_AUDIT.md#10-breaking-changes-made-during-this-audit-pre-01).

## Per-crate verdicts

### `essenty-lifecycle`

`LifecycleState` (ordered enum + `is_resumed/is_started/is_destroyed`),
`LifecycleError` (`InvalidTransition{from,to}`, `AlreadyDestroyed`),
`LifecycleRegistry` (`new/state/subscriber_count/subscribe/unsubscribe/move_to/
create/start/resume/pause/stop/destroy`, `Clone`-shared, `Default`),
`Subscription` (RAII drop, `unsubscribe`, `detach`→id, `id()`, `is_active`),
new `do_on_create/do_on_start[_once]/do_on_resume[_once]/do_on_pause[_once]/
do_on_stop[_once]/do_on_destroy`. Necessary: yes. Idiomatic: yes (RAII guards,
typed errors, direction-tracked helpers). `!Send/!Sync` via `Rc<RefCell>`
documented in crate docs. No platform leakage. No Owner trait — intentional
(see `DECOMPOSE_READINESS.md`). Internal `rank/from_rank` correctly private.

### `essenty-state-keeper`

`StateKeeper` (`new/with_restored/has_provider/is_registered/provider_count/
pending_count/register/register_value/register_optional/register_optional_value/
unregister/consume_bytes/consume_value/save`), `StateKeeperError`
(`DuplicateKey/Encode{key,reason}/Decode{key,reason}`). Byte-first with opt-in
serde helpers is the right layering; `serde` appears in signatures only as
trait bounds plus explicit caller-supplied encode/decode closures — no format
lock-in. `BTreeMap` in `save`/`with_restored` signatures exposes ordering as a
feature, not an accident. `!Send/!Sync` not universally guaranteed (provider
trait objects) — documented.

### `essenty-instance-keeper`

`InstanceKeeper` (`new/len/is_empty/is_destroyed/contains/get_or_create/get/
put/remove/destroy/clear/destroy_all`), `InstanceKeeperError`
(`TypeMismatch/DuplicateKey/Destroyed`). `Rc<T>` ownership + `Drop` cleanup is
documented at crate level. `put` returns `Rc<T>` (shared ownership, no clone
needed). `remove` returns `Option<Rc<T>>` and re-inserts on type mismatch.
`destroy(key)->bool` vs `remove(key)->Result` overlap is justified: destroy is
the fire-and-forget release, remove hands ownership back. No Owner trait —
intentional.

### `essenty-back-handler`

`BackDispatcher` (`new/handler_count/can_handle/has_active_gesture/register/
register_reentrant/unregister/set_enabled/is_enabled/set_priority/priority/back/
predictive_start[_with]/predictive_progress[_with]/predictive_cancel/
predictive_invoke/add_enabled_changed_listener/remove_enabled_changed_listener`),
`BackHandle` (opaque id holder), `BackCommands` (reentrancy queue),
`BackEvent/BackPhase/GesturePosition/SwipeEdge`, `BackError`
(`NoGestureInProgress`). `!Send/!Sync` (callback trait objects) documented.
`BackCommands` registering during dispatch with immediate-after-return
application is the correct answer to aliasing. No Android types leak.

### `essenty` (facade + optional runtime)

Re-exports are flat and complete for the four primitives. `runtime` feature
gates `Runtime/PlatformEvent/DispatchResult/RuntimeError`; default `essenty`
pulls no platform or async code (**KEEP** verdict: optional convenience host,
nothing in the workspace depends on it, must stay minimal and must not grow
toward component responsibilities — see below). `Runtime::dispatch` ties
lifecycle-destroy to `instance_keeper.destroy_all()`, which is the sane
default for a single-scope host.

### `essenty-android`

`AndroidLifecycle`, `AndroidStateHost`, `AndroidBackBridge`,
`NativeActivityLifecycle`, `NativeActivityState` (+ `encode/decode_native_state`,
`NativeStateError`), `native_config` (`NativeConfigCategory`, manifest
rendering/parsing, `HostConfigurationReport`, `inspect_host_configuration`),
`AndroidBackHandler` + `AndroidBackStrategy` + `AndroidBackError`
(android-gated). JNI/`android-activity` stay behind `cfg(target_os="android")`
+ `native-activity` feature — verified via `cargo tree -e features` that plain
`essenty` pulls none of it. `back_handler/platform.rs` carries
`#![allow(unsafe_code)]` at module scope; the workspace `unsafe_code = "deny"`
lint otherwise holds — the allow is localized and each block documents its
invariant (see Unsafe section).

### `essenty-apple`

`AppleLifecycle` (shared mapping, all targets), per-OS `ApplicationLifecycle`
(macOS / UIKit(iOS+tvOS+visionOS) / watchOS) + `ApplicationLifecycleError`
(`NotMainThread`), `shared` module. Crate-level `unsafe_code = "allow"` with
per-block SAFETY comments; `MainThreadOnly` confinement documented. WatchKit
`unsafe extern` statics are framework-owned constant strings — reviewed, safe
as declared.

### `essenty-web`

`VisibilityLifecycle`, `PageVisibility`, `HistoryBackBridge`, `StorageKey`,
`StorageArea`, wasm-only `wasm::{BrowserLifecycle,BrowserHistoryBack,
BrowserStorage}`. `wasm-bindgen/js-sys/web-sys` never enter core crates.
Hex (not base64) storage encoding is an explicit no-opinion choice documented
in `storage.rs`.

## Feature flags

| Feature | Default | Pulls in | Verdict |
|---|---|---|---|
| `essenty/runtime` | off | `thiserror` only | KEEP — no executor, no platform |
| `essenty-android/native-activity` | off | `android-activity`, `jni`, `jni-min-helper`, `log` (all android-gated) | KEEP — correctly optional; `cargo add essenty` never touches JNI |
| everything else | — | no other features exist | No accidental coupling; `--all-features` workspace build is clean |

## Dependencies

- `serde` (state-keeper, `default-features=false`, `derive+alloc`): justified;
  bounds-only in public API, caller-chosen codecs. `serde_json` is
  dev-dependency-only (tests/docs). No JSON lock-in.
- `thiserror` (all cores + optional runtime): justified; typed errors are part
  of the public contract.
- `android-activity 0.6.1`, `jni 0.22.4`, `jni-min-helper 0.4.7`, `log 0.4`:
  android-gated + feature-gated. Maintenance risk noted (0.x JNI stack) but
  contained to one adapter crate.
- `objc2` family (`objc2`, `objc2-foundation`, `objc2-app-kit`/`objc2-ui-kit`
  per-OS): correctly split per target OS; version-pinned in lockfile.
- `wasm-bindgen 0.2`, `js-sys`, `web-sys` (narrowed features): web-crate only.
- No dependency leaks platform types into core signatures (verified by
  inspection of all four core `lib.rs` re-export lists).

## Unsafe / FFI

Core crates (`lifecycle`, `state-keeper`, `instance-keeper`, `back-handler`,
`essenty`): zero `unsafe`, zero `extern` — workspace deny-lint enforced.
Platform `unsafe` inventory: Apple `define_class!`/`msg_send!`/observer
register+`removeObserver` in `Drop` (main-thread confined, documented);
watchOS `unsafe extern "C"` WatchKit notification-name statics (framework-owned,
read-only); Android `JavaVM::from_raw` + `as_cast_raw` in back
proxy/config readout (module-scoped allow, panic boundaries via
`catch_unwind`, pending-exception clearing documented in `ANDROID.md`).
Invariants, thread assumptions, and cleanup were reviewed per block; no core
state machine needs unsafe.

## Threading

All four cores are intentionally single-threaded (`Rc`/`RefCell`/`FnMut`):
`LifecycleRegistry`/`Subscription`, `InstanceKeeper`, `StateKeeper`
(provider trait objects give no global guarantee), `BackDispatcher` are
`!Send`/`!Sync` by construction, documented in crate docs. Android callback
queues cross threads via `mpsc` + `Mutex` with drain-on-event-loop; Apple
observers are `MainThreadOnly`; web closures are wasm-main-thread by
platform. No `Send`/`Sync` was added for convenience. Multithreaded hosts
confine or wrap — Decompose-rs must do the same (single-threaded component
model, see `DECOMPOSE_READINESS.md`).

## Error model

Typed `thiserror` errors everywhere; intentional differences from Kotlin
exceptions recorded per module in `UPSTREAM_PARITY_AUDIT.md` (strict
transitions, `AlreadyDestroyed`, `bool` unregister, `NoGestureInProgress`).
No panics on public paths (only `u64::MAX` sentinel on detached-guard misuse,
documented). Provider/encode/decode failures propagate with key context.
`NativeStateError` covers malformed/oversize/version/checksum + provider
errors. No swallowed failures found.

## Reentrancy / determinism / memory

- Reentrancy: lifecycle snapshots observers per event + `move_to` re-reads
  state (nested transitions safe); back mutations queue via `BackCommands`;
  state `save(&self)` + `register(&mut self)` exclude save-time mutation by
  construction; instance creation during creation is plain reentrant `&mut`
  code (factory runs before insert — recursive `get_or_create` on the same
  keeper would borrow-check at runtime? `get_or_create` takes `&mut self` and
  calls `create()` while holding no borrow — actually `self.instances.get`
  borrow ends before `create()` runs (NLL), so recursion panics only on
  `RefCell`-style misuse, of which there is none (`BTreeMap` direct, no
  interior mutability). Documented as safe-by-construction.
- Determinism: `BTreeMap` ordering for state save, instance storage, back
  entries (winner by `(priority, id)` — id order is registration order, no
  hash iteration anywhere in semantic paths). Lifecycle ordering
  forward-vs-reverse explicitly tested.
- Memory: RAII unsubscription, destroy-clears-observers, keeper `Drop`
  semantics tested (`value_drops_when_keeper_and_clones_are_gone`,
  `keeper_drop_releases_retained_values`); Android proxy teardown + retry
  fallback documented; Apple `removeObserver` in `Drop`; web closures owned by
  wasm bindings with documented teardown.

## no_std / alloc

Not a goal of this task. Assessment per core crate: `lifecycle` —
`alloc`-only feasible (`Rc`, `Vec` snapshot); `state-keeper` — feasible
 modulo `std` error-trait imports and serde `alloc` config; `instance-keeper`
 — blocked by `std::any::Any`/`Rc` imports (mechanical); `back-handler` —
 feasible (`BTreeMap`, `Box`). Platform adapters fundamentally need their
 OS runtimes. No API was distorted for `no_std`; all crates remain `std`
 without claiming otherwise.

## Performance

No optimization performed; no perf claims made. Hot paths identified for the
existing `BENCHMARK_PLAN.md` baseline work: lifecycle dispatch (snapshot
`Vec` per event — proportional to observer count, fine), state save (clone of
restored map + provider eval), instance lookup (`BTreeMap` log-n),
back dispatch (linear winner scan — proportional to handler count, fine for
UI-scale registrations), JNI chatter (queued + drained per loop, no
per-callback crossings). No quadratic behavior, no repeated serialization,
no accidental cloning on hot paths.

## Package metadata / MSRV / docs / examples

- Metadata: all 8 crates carry workspace-inherited description/repository/
  homepage/license/keywords/categories; version `0.1.0`, edition 2024,
  `rust-version = "1.85"`. Effectively publishing-ready except CHANGELOG/README
  per crate — acceptable pre-0.1, no publish performed.
- MSRV/toolchain: `rust-version = 1.85`, edition 2024; CI tracks stable.
  No MSRV advertised beyond the manifest field; verified with current stable
  in this audit.
- Documentation: crate-level docs + platform guides + parity docs
  cross-linked; `cargo doc --workspace --no-deps` clean.
- Examples: `crates/essenty/examples/counter.rs` (end-to-end core demo),
  `essenty-android/examples/native_state.rs`,
  `essenty-apple/examples/macos_notifications.rs`; `examples/README.md`.
  Each major primitive is exercised by doctests plus the counter demo;
  platform examples require their SDKs by nature. No example exposes
  internal patterns users should not copy.
