//! Tokio integration for [`essenty_lifecycle`].
//!
//! # Why this crate exists
//!
//! Upstream Essenty ships two Kotlin-only integrations:
//!
//! - `lifecycle-coroutines` (`CoroutineScopeWithLifecycle`, `repeatOnLifecycle`,
//!   `Flow.withLifecycle`, Main-dispatcher helpers)
//! - `lifecycle-reaktive` (`DisposableScope`, `Disposable.withLifecycle`)
//!
//! Both solve the same practical problem: run asynchronous work that
//! automatically follows an Essenty [`Lifecycle`](essenty_lifecycle::LifecycleRegistry).
//! The Kotlin type system (coroutine scopes, Flows, Reaktive disposables) is not
//! portable to Rust, so this crate ports the *capability*, not the API:
//!
//! - task/scope cancelled on `DESTROYED` → [`LifecycleScope`]
//! - work starts when the lifecycle reaches an active state, restarts on
//!   re-entry, and ends permanently on `DESTROYED` → [`repeat_on_lifecycle`]
//! - subscriptions/resources disposed when the lifecycle ends → task
//!   cancellation plus RAII [`Subscription`](essenty_lifecycle::Subscription)
//!   guards (no reactive framework ported)
//!
//! # Relationship to upstream
//!
//! | Upstream | Rust status |
//! |---|---|
//! | `CoroutineScopeWithLifecycle` / `coroutineScope()` / `withLifecycle` | Implemented as [`LifecycleScope`]. Destroy cancels every owned task; spawning after destroy is a typed error. |
//! | `Lifecycle.repeatOnLifecycle` | Implemented as [`repeat_on_lifecycle`]. Same restart/cancel semantics over `CREATED`/`STARTED`/`RESUMED`; `INITIALIZED` is rejected; `DESTROYED` returns immediately. |
//! | `Flow.withLifecycle` | Deferred (see below). Finish scope + repeat first. |
//! | `Dispatchers.Main.immediateOrFallback` | Intentionally absent. Tokio has a different execution model; UI-main-thread dispatch belongs to a platform/UI crate, not this integration. |
//! | `DisposableScope` / `Disposable.withLifecycle` | Not ported as an API. Covered by task cancellation, lifecycle subscriptions, and `Drop`. |
//!
//! # `LifecycleScope` usage
//!
//! ```rust,no_run
//! use essenty_lifecycle::LifecycleRegistry;
//! use essenty_lifecycle_tokio::LifecycleScope;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let lifecycle = LifecycleRegistry::new();
//! let scope = LifecycleScope::new(lifecycle.clone());
//!
//! scope.spawn(async { println!("loaded"); }).unwrap();
//!
//! lifecycle.create().unwrap();
//! lifecycle.destroy().unwrap(); // aborts the task above
//! # }
//! ```
//!
//! For explicit runtime control (more predictable in tests), use
//! [`LifecycleScope::with_handle`] with a cloned
//! [`tokio::runtime::Handle`].
//!
//! # `repeat_on_lifecycle` usage
//!
//! ```rust,no_run
//! use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
//! use essenty_lifecycle_tokio::repeat_on_lifecycle;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let lifecycle = LifecycleRegistry::new();
//! let driver = lifecycle.clone();
//! let repeater = tokio::task::spawn_local(async move {
//!     repeat_on_lifecycle(lifecycle, LifecycleState::Started, || async {
//!         println!("observing while started");
//!     })
//!     .await
//!     .unwrap();
//! });
//!
//! driver.create().unwrap();
//! driver.start().unwrap();
//! driver.stop().unwrap();
//! driver.destroy().unwrap();
//! repeater.await.unwrap();
//! # }
//! ```
//!
//! The block factory is invoked anew on every entry into the active region;
//! futures are never paused and resumed. Two instances never run concurrently:
//! the previous child is aborted and awaited before a new one starts.
//!
//! # Cancellation semantics
//!
//! - Leaving the active region aborts the current child task, then awaits its
//!   `JoinHandle` so rapid `START → STOP → START` sequences cannot overlap.
//! - `DESTROYED` aborts the child and completes the [`repeat_on_lifecycle`]
//!   future permanently; later transitions never restart it.
//! - Dropping the [`repeat_on_lifecycle`] future (external cancellation)
//!   unsubscribes the lifecycle observer and aborts the running child.
//! - Dropping a [`LifecycleScope`] aborts every owned task and removes its
//!   destroy subscription; no detached tasks are left behind.
//! - Every lifecycle subscription is RAII-guarded and removed on scope drop,
//!   repeat completion/cancellation, destroy, or setup failure.
//!
//! # Tokio runtime requirement
//!
//! This crate never creates a runtime. [`LifecycleScope::new`] uses
//! [`tokio::runtime::Handle::current`], [`LifecycleScope::with_handle`] uses
//! the supplied handle, and [`repeat_on_lifecycle`] spawns children on the
//! ambient runtime via [`tokio::spawn`]. Callers must poll or await inside a
//! Tokio runtime; otherwise Tokio panics. Prefer passing an explicit `Handle`
//! in libraries and tests.
//!
//! # Send constraints
//!
//! The core registry stays single-threaded (`!Send`/`!Sync`); this crate does
//! not change that. [`LifecycleScope`] and the [`repeat_on_lifecycle`] future
//! are therefore `!Send` and must live on the lifecycle-owning thread (for
//! example the current-thread runtime or a `LocalSet`). Stricter bounds stay
//! local: [`LifecycleScope::spawn`] and the repeat block futures require
//! `Future + Send + 'static` because Tokio worker threads execute them. The
//! repeat block *factory* itself may capture `!Send` data; only the future it
//! returns must be `Send`.
//!
//! # `spawn_local` status
//!
//! `spawn_local: deferred.` Tokio local tasks need a `LocalSet` owner, and
//! hiding that ownership inside the scope would add global machinery for
//! little value. `!Send` async work (for example `Rc`/`RefCell` component
//! state) should use `tokio::task::LocalSet::spawn_local` directly alongside
//! the lifecycle until a concrete Decompose-rs need arrives.
//!
//! # Stream integration status
//!
//! No `with_lifecycle(Stream)` adapter is provided. Upstream `Flow.withLifecycle`
//! is a thin wrapper over `repeatOnLifecycle`; the same shape can be added later
//! over `futures_core::Stream` if a concrete consumer needs it. Scope +
//! repeat cover the lifecycle-aware async capability without a new framework.
//!
//! # No Main dispatcher
//!
//! Upstream defaults to `Dispatchers.Main` because Kotlin UI environments need
//! it. Tokio has no equivalent ambient UI dispatcher, so this crate provides
//! none. Platform UI-main-thread dispatch belongs to a future platform/UI
//! integration, not here.

mod error;
mod repeat;
mod scope;

pub use error::{RepeatError, SpawnError};
pub use repeat::repeat_on_lifecycle;
pub use scope::LifecycleScope;
