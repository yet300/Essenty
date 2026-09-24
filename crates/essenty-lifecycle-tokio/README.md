# essenty-lifecycle-tokio

Tokio integration for `essenty-lifecycle`: lifecycle-bound async work without
porting Kotlin coroutine or reactive type systems.

## Why

Upstream Essenty solves a practical problem twice — `lifecycle-coroutines`
(`CoroutineScopeWithLifecycle`, `repeatOnLifecycle`, `Flow.withLifecycle`) and
`lifecycle-reaktive` (`DisposableScope`, `Disposable.withLifecycle`): run
asynchronous work that automatically follows a `Lifecycle`. The Kotlin
machinery (coroutine scopes, Flows, Reaktive disposables, `Dispatchers.Main`)
is language-specific. This crate ports the *capability* using Tokio directly:

- [`LifecycleScope`](src/scope.rs) — spawn tasks that are cancelled on
  `DESTROYED` (covers `CoroutineScopeWithLifecycle`).
- [`repeat_on_lifecycle`](src/repeat.rs) — restart work on every entry into
  an active state, cancel on exit, finish permanently on `DESTROYED` (covers
  `Lifecycle.repeatOnLifecycle`).
- Reaktive's dispose-on-destroy is covered by task cancellation plus RAII
  `Subscription` guards; no reactive framework was ported.

Core crates stay Tokio-free. Only this crate depends on Tokio, with minimal
features (`rt`, `sync`; test-only `macros`, `time`, multi-thread runtime).

## Usage

```rust,no_run
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use essenty_lifecycle_tokio::{LifecycleScope, repeat_on_lifecycle};

# #[tokio::main(flavor = "current_thread")]
# async fn main() {
// Scope: tasks die with the lifecycle (or with the scope).
let lifecycle = LifecycleRegistry::new();
let scope = LifecycleScope::new(lifecycle.clone());
scope.spawn(async { println!("loaded"); }).unwrap();

// Repeat: new future per entry, cancelled per exit, done on destroy.
repeat_on_lifecycle(lifecycle, LifecycleState::Started, || async {
    println!("observing while started");
})
.await
.unwrap();
# }
```

Prefer `LifecycleScope::with_handle(lifecycle, handle)` with an explicit
`tokio::runtime::Handle` in libraries and tests.

## Semantics

- Leaving the active region aborts the child, then awaits its `JoinHandle`,
  so rapid `START → STOP → START` bursts never overlap (an `mpsc` queue
  preserves every transition; `watch` would coalesce the stop away).
- `DESTROYED` aborts the child and completes `repeat_on_lifecycle`
  permanently; `INITIALIZED` is rejected as a minimum state; an already
  destroyed lifecycle resolves immediately without work.
- Dropping a `LifecycleScope` aborts owned tasks and removes its destroy
  subscription. Dropping (externally cancelling) the repeat future
  unsubscribes and aborts the running child. No detached tasks leak.

## Constraints

- **Runtime:** this crate never creates one. `new` uses `Handle::current`,
  `with_handle` uses yours, repeat spawns on the ambient runtime. Poll inside
  Tokio or Tokio panics.
- **Threading:** core stays `!Send`/`!Sync`; scope and repeat future are
  `!Send` and live on the lifecycle-owning thread (current-thread runtime or
  `LocalSet`). Spawned futures and repeat block futures are `Send + 'static`;
  the block *factory* may capture `!Send` data.
- **`spawn_local`: deferred.** Local tasks need a `LocalSet` owner; hiding
  that inside the scope would add global machinery. Use
  `LocalSet::spawn_local` directly for `Rc`/`RefCell` work.
- **Streams: deferred.** No `with_lifecycle(Stream)` adapter; scope + repeat
  cover the capability. Upstream `Flow.withLifecycle` is a thin wrapper over
  `repeatOnLifecycle` and can be added if a consumer needs it.
- **No Main dispatcher.** Tokio has no UI-main equivalent; platform dispatch
  belongs to a future UI integration, not here.
