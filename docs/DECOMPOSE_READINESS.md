# Decompose-rs foundation readiness

Question: can a future Rust equivalent of `ComponentContext` be built from
`Lifecycle` + `StateKeeper` + `InstanceKeeper` + `BackHandler` without
platform types, global singletons, mandatory `Send`+`Sync`, Android-specific
ownership, executor lock-in, JNI/Objective-C/browser types? **Yes.**

(No `ComponentContext` API is defined here — sketches below are analysis
only, per the audit's scope boundary.)

## What a ComponentContext needs vs. what exists

| Need | Essenty-rs supply | Gap |
|---|---|---|
| Owned lifecycle per component | `LifecycleRegistry` (`Clone`-shared handle, RAII `Subscription`, `do_on_*` incl. `do_on_destroy` for cleanup) | None |
| Saved state per component | `StateKeeper` (register/consume/save, optional providers, typed errors, deterministic snapshots) | None — key namespacing is the compositor's job |
| Retained instances per component | `InstanceKeeper` (`get_or_create`/`put`/`get`/`remove`/`destroy`, `Drop` cleanup, `destroy_all` at scope end) | None |
| Back handling per component | `BackDispatcher` (priority + enabled + predictive gestures + aggregate listeners for native sync) | Parent/child propagation is Decompose-rs design work (upstream has no such abstraction either) |
| Lifecycle-bound cleanup (coroutine cancel, disposable dispose) | `do_on_destroy` + RAII, plus optional `essenty-lifecycle-tokio` (`LifecycleScope`, `repeat_on_lifecycle`) | None — core stays executor-neutral; Tokio lives only in the optional integration |

## Constraints check (all satisfied)

- No platform types in core signatures: verified across all four core
  `lib.rs` re-export lists; JNI/Objective-C/wasm stay in adapter crates.
- No global singleton: every primitive is constructed per-owner (`new()` /
  `with_restored`); `Runtime` is an owned value, not a static.
- No mandatory `Send`+`Sync`: cores are `!Send`/`!Sync` by construction;
  Decompose-rs must adopt a single-threaded component model (or explicit
  confinement), matching upstream's main-thread reality.
- No Android-specific ownership: retention is `Rc`-local, state is opaque
  bytes; the NativeActivity contract (`PLATFORM_PARITY.md`) is one host
  binding among three, not a core assumption.
- No executor lock-in: core is synchronous; the optional Tokio bridge
  (`LifecycleScope` destroy→cancel, `repeat_on_lifecycle` start/stop relaunch)
  is expressible without new core APIs, and a component can own a
  `LifecycleScope` so its async tasks die with its lifecycle.

## Ownership sketch (thought experiment, not committed API)

A component context would own the four primitives (or hold the shared
handles: `LifecycleRegistry` is already `Clone`-shared; the other three sit
behind `&mut` or a confining wrapper) and fan platform events out. The
existing optional `Runtime` demonstrates the fan-out shape but must NOT become
the component model: it is a single-scope convenience host, and component
trees need per-node lifecycles/keepers, not one root bundle.

## Blockers

None architectural. Non-blocking tracked work: nested back-dispatcher
composition design (Decompose-rs scope), simulator/device/browser runtime
delivery (environment-limited, implementation reviewed), executor-neutral async
bridge proposal (future, unblocks nothing in the foundation).

```text
ESSENTY-RS STATUS:
READY FOR DECOMPOSE-RS
```
