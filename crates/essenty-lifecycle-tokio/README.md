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
  `DESTROYED` (covers `CoroutineScopeWithLifecycle`), via `spawn` for `Send`
  work or `spawn_local` for thread-local work.
- [`repeat_on_lifecycle`](src/repeat.rs) — restart work on every entry into
  an active state, cancel on exit, finish permanently on `DESTROYED` (covers
  `Lifecycle.repeatOnLifecycle`), plus [`repeat_on_lifecycle_local`](src/repeat.rs)
  with identical semantics for thread-local futures.
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
let scope = LifecycleScope::try_new(lifecycle.clone()).unwrap();
scope.spawn(async { println!("loaded"); }).unwrap();

// Repeat: new future per entry, cancelled per exit, done on destroy.
repeat_on_lifecycle(lifecycle, LifecycleState::Started, || async {
    println!("observing while started");
})
.await
.unwrap();
# }
```

Thread-local component state (`Rc`, `RefCell`, …) uses the matching local
APIs inside a `LocalSet` — same tracking, same restart semantics, no `Send`
bound:

```rust,no_run
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use essenty_lifecycle_tokio::{LifecycleScope, repeat_on_lifecycle_local};
use std::cell::RefCell;
use std::rc::Rc;

# #[tokio::main(flavor = "current_thread")]
# async fn main() {
let lifecycle = LifecycleRegistry::new();
let local = tokio::task::LocalSet::new();
local
    .run_until(async {
        let scope = LifecycleScope::new(lifecycle.clone());
        let component = Rc::new(RefCell::new(0_u32));
        scope
            .spawn_local(async move {
                *component.borrow_mut() += 1;
                tokio::task::yield_now().await;
                *component.borrow_mut() += 1;
            })
            .unwrap();
        repeat_on_lifecycle_local(lifecycle, LifecycleState::Started, || {
            let component = Rc::clone(&component);
            async move {
                component.borrow_mut().push(1_u32);
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    })
    .await;
# }
```

Prefer `LifecycleScope::with_handle(lifecycle, handle)` with an explicit
`tokio::runtime::Handle` in libraries and tests; `try_new` is the
non-panicking ambient alternative (`new` panics outside a runtime).

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

- **Runtime:** this crate never creates one and never creates a `LocalSet`.
  `new` uses `Handle::current` (panics outside a runtime); `try_new` returns
  a typed `ScopeError` instead; `with_handle` uses yours. The `Send` repeat
  spawns on the ambient runtime; local APIs require an ambient `LocalSet` or
  local runtime and propagate Tokio's panic otherwise (Tokio offers no
  non-panicking probe for that context). Poll inside Tokio or Tokio panics.
- **Threading:** core stays `!Send`/`!Sync`; scope and both repeat futures are
  `!Send` and live on the lifecycle-owning thread (current-thread runtime or
  `LocalSet`). `Send` and local work are separate explicit APIs, not one
  generic bound: `spawn` / `repeat_on_lifecycle` take `Send + 'static`
  futures, `spawn_local` / `repeat_on_lifecycle_local` take
  `Future + 'static` and run on the driving thread.
- **Streams: deferred.** No `with_lifecycle(Stream)` adapter; scope + repeat
  cover the capability. Upstream `Flow.withLifecycle` is a thin wrapper over
  `repeatOnLifecycle` and can be added if a consumer needs it.
- **No Main dispatcher.** Tokio has no UI-main equivalent; platform dispatch
  belongs to a future UI integration, not here.
