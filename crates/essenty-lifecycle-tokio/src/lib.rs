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
//! - task/scope cancelled on `DESTROYED` → [`LifecycleScope`], via
//!   [`spawn`](LifecycleScope::spawn) for `Send` work or
//!   [`spawn_local`](LifecycleScope::spawn_local) for thread-local work
//! - work starts when the lifecycle reaches an active state, restarts on
//!   re-entry, and ends permanently on `DESTROYED` → [`repeat_on_lifecycle`]
//!   (`Send`) or [`repeat_on_lifecycle_local`] (thread-local)
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
//! [`tokio::runtime::Handle`], or [`LifecycleScope::try_new`] when the ambient
//! runtime may be absent.
//!
//! Thread-local work uses the matching local APIs inside a
//! [`LocalSet`](tokio::task::LocalSet):
//!
//! ```rust,no_run
//! use essenty_lifecycle::LifecycleRegistry;
//! use essenty_lifecycle_tokio::LifecycleScope;
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let lifecycle = LifecycleRegistry::new();
//! let local = tokio::task::LocalSet::new();
//! local
//!     .run_until(async {
//!         let scope = LifecycleScope::new(lifecycle.clone());
//!         let component = Rc::new(RefCell::new(0_u32));
//!         scope
//!             .spawn_local(async move {
//!                 *component.borrow_mut() += 1;
//!                 tokio::task::yield_now().await;
//!                 *component.borrow_mut() += 1;
//!             })
//!             .unwrap();
//!     })
//!     .await;
//! # }
//! ```
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
//! [`repeat_on_lifecycle_local`] provides identical semantics for
//! thread-local (`!Send`) block futures.
//!
//! # Cancellation semantics
//!
//! - Leaving the active region aborts the current child task, then awaits its
//!   `JoinHandle` so rapid `START → STOP → START` sequences cannot overlap.
//! - `DESTROYED` aborts the child and completes the repeat future permanently;
//!   later transitions never restart it.
//! - Dropping a repeat future (external cancellation) unsubscribes the
//!   lifecycle observer and aborts the running child.
//! - Dropping a [`LifecycleScope`] aborts every owned task — `Send` and local
//!   alike — and removes its destroy subscription; no detached tasks are left
//!   behind.
//! - Every lifecycle subscription is RAII-guarded and removed on scope drop,
//!   repeat completion/cancellation, destroy, or setup failure.
//!
//! # Tokio runtime requirement
//!
//! This crate never creates a runtime and never creates a `LocalSet`.
//! [`LifecycleScope::new`] uses [`tokio::runtime::Handle::current`] and panics
//! outside a runtime; [`LifecycleScope::try_new`] returns a typed
//! [`ScopeError`] instead, and
//! [`LifecycleScope::with_handle`] takes an explicit handle. The `Send` repeat
//! spawns children on the ambient runtime via [`tokio::spawn`]; the local
//! repeat and [`spawn_local`](LifecycleScope::spawn_local) require an ambient
//! [`LocalSet`](tokio::task::LocalSet) or local runtime and propagate Tokio's
//! panic otherwise (Tokio offers no non-panicking probe for that context).
//! Prefer explicit handles in libraries and tests.
//!
//! # `Send` and local APIs
//!
//! The core registry stays single-threaded (`!Send`/`!Sync`); this crate does
//! not change that. [`LifecycleScope`] and both repeat futures are therefore
//! `!Send` and must live on the lifecycle-owning thread (for example the
//! current-thread runtime or a `LocalSet`). The two flavors are separate
//! explicit APIs rather than one generic bound:
//!
//! | Work | Scope | Repeat |
//! |---|---|---|
//! | `Send + 'static` futures | [`spawn`](LifecycleScope::spawn) | [`repeat_on_lifecycle`] |
//! | Thread-local futures (`Rc`, `RefCell`, …) | [`spawn_local`](LifecycleScope::spawn_local) (inside a `LocalSet`) | [`repeat_on_lifecycle_local`] (inside a `LocalSet`) |
//!
//! The repeat block *factory* itself may capture `!Send` data in both flavors;
//! only the future it returns has the flavor's bound.
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

pub use error::{RepeatError, ScopeError, SpawnError};
pub use repeat::{repeat_on_lifecycle, repeat_on_lifecycle_local};
pub use scope::LifecycleScope;
