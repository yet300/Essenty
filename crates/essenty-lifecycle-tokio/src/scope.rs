use crate::{ScopeError, SpawnError};
use essenty_lifecycle::{LifecycleRegistry, LifecycleState, Subscription};
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use tokio::runtime::Handle;
use tokio::task::{AbortHandle, JoinHandle};

struct ScopeInner {
    lifecycle: LifecycleRegistry,
    handle: Handle,
    tasks: RefCell<Vec<AbortHandle>>,
}

impl std::fmt::Debug for ScopeInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopeInner")
            .field("lifecycle_state", &self.lifecycle.state())
            .field("task_count", &self.tasks.borrow().len())
            .finish_non_exhaustive()
    }
}

/// Tokio-aware lifecycle scope.
///
/// Owns every task spawned through it and cancels them when the lifecycle is
/// destroyed or when the scope itself is dropped.
///
/// The scope is `!Send`/`!Sync` by construction: it holds the
/// single-threaded [`LifecycleRegistry`] and its RAII [`Subscription`], so it
/// must live on the lifecycle-owning thread. Two task flavors are offered as
/// separate explicit APIs: [`spawn`](Self::spawn) for `Send` futures and
/// [`spawn_local`](Self::spawn_local) for thread-local (`!Send`) futures such
/// as component state behind `Rc`/`RefCell`. Both are tracked and cancelled
/// identically.
///
/// ```rust,no_run
/// use essenty_lifecycle::LifecycleRegistry;
/// use essenty_lifecycle_tokio::LifecycleScope;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let lifecycle = LifecycleRegistry::new();
/// let scope = LifecycleScope::new(lifecycle.clone());
/// scope.spawn(async { println!("work"); }).unwrap();
/// lifecycle.destroy().unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct LifecycleScope {
    inner: Rc<ScopeInner>,
    _subscription: Subscription,
}

impl LifecycleScope {
    /// Creates a scope bound to `lifecycle`, spawning through the ambient
    /// Tokio runtime.
    ///
    /// This is a panicking convenience for application code that is known to
    /// run inside a runtime. Prefer [`try_new`](Self::try_new) when the
    /// runtime context is uncertain, or [`with_handle`](Self::with_handle)
    /// for explicit control in libraries and tests.
    ///
    /// If `lifecycle` is already destroyed, the scope is created inert:
    /// every [`spawn`](Self::spawn) and [`spawn_local`](Self::spawn_local)
    /// returns [`SpawnError::Destroyed`].
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime (propagates
    /// [`Handle::current`]'s panic).
    #[must_use]
    pub fn new(lifecycle: LifecycleRegistry) -> Self {
        Self::with_handle(lifecycle, Handle::current())
    }

    /// Creates a scope bound to `lifecycle`, spawning through the ambient
    /// Tokio runtime, without panicking when no runtime is active.
    ///
    /// This is the idiomatic constructor when the caller cannot guarantee a
    /// runtime context. No runtime is ever created implicitly.
    ///
    /// If `lifecycle` is already destroyed, the scope is created inert:
    /// every [`spawn`](Self::spawn) and [`spawn_local`](Self::spawn_local)
    /// returns [`SpawnError::Destroyed`].
    ///
    /// # Errors
    ///
    /// Returns [`ScopeError::NoRuntime`] when called outside a Tokio runtime.
    pub fn try_new(lifecycle: LifecycleRegistry) -> Result<Self, ScopeError> {
        Ok(Self::with_handle(lifecycle, Handle::try_current()?))
    }

    /// Creates a scope bound to `lifecycle`, spawning through `handle`.
    ///
    /// Using an explicit handle makes ownership obvious and tests deterministic
    /// (pass `Handle::current()` from the test runtime). No runtime is ever
    /// created implicitly.
    ///
    /// If `lifecycle` is already destroyed, the scope is inert and every
    /// [`spawn`](Self::spawn) returns [`SpawnError::Destroyed`].
    #[must_use]
    pub fn with_handle(lifecycle: LifecycleRegistry, handle: Handle) -> Self {
        let inner = Rc::new(ScopeInner { lifecycle, handle, tasks: RefCell::new(Vec::new()) });
        let aborter = Rc::clone(&inner);
        let subscription = inner.lifecycle.do_on_destroy(move || {
            let handles = std::mem::take(&mut *aborter.tasks.borrow_mut());
            for handle in handles {
                handle.abort();
            }
        });
        Self { inner, _subscription: subscription }
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> LifecycleState {
        self.inner.lifecycle.state()
    }

    /// Returns `true` once the lifecycle has reached `DESTROYED`.
    #[must_use]
    pub fn is_destroyed(&self) -> bool {
        self.inner.lifecycle.state().is_destroyed()
    }

    /// Returns the Tokio handle tasks are spawned through.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.inner.handle.clone()
    }

    /// Spawns `future` on the scope's runtime and tracks it for cancellation.
    ///
    /// The task may run while the lifecycle is alive. It is aborted when the
    /// lifecycle is destroyed or when this scope is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError::Destroyed`] without spawning if the lifecycle is
    /// already destroyed.
    pub fn spawn<T>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Result<JoinHandle<T>, SpawnError>
    where
        T: Send + 'static,
    {
        self.check_alive()?;
        let join = self.inner.handle.spawn(future);
        self.track(join.abort_handle());
        Ok(join)
    }

    /// Spawns a thread-local (`!Send`) `future` on the ambient
    /// [`LocalSet`](tokio::task::LocalSet) and tracks it for cancellation
    /// exactly like [`spawn`](Self::spawn).
    ///
    /// This is the route for component state that cannot cross threads
    /// (`Rc`, `RefCell`, …). The future requires only `Future + 'static` —
    /// no `Send` bound. It runs on the thread that drives the `LocalSet`,
    /// which must be the lifecycle-owning thread holding this scope.
    ///
    /// The scope creates no `LocalSet` itself: call this (and poll the
    /// returned handle) from inside `LocalSet::run_until` or a
    /// [`tokio::runtime::LocalRuntime`].
    ///
    /// # Errors
    ///
    /// Returns [`SpawnError::Destroyed`] without spawning if the lifecycle is
    /// already destroyed.
    ///
    /// # Panics
    ///
    /// Panics if called outside a [`LocalSet`](tokio::task::LocalSet) or local
    /// runtime (propagates [`tokio::task::spawn_local`]'s panic). Tokio offers
    /// no non-panicking probe for the local-task context, so no typed error
    /// is possible here; the contract matches `spawn_local` itself.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use essenty_lifecycle::LifecycleRegistry;
    /// use essenty_lifecycle_tokio::LifecycleScope;
    /// use std::cell::RefCell;
    /// use std::rc::Rc;
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let lifecycle = LifecycleRegistry::new();
    /// let local = tokio::task::LocalSet::new();
    /// local
    ///     .run_until(async {
    ///         let scope = LifecycleScope::new(lifecycle.clone());
    ///         let component = Rc::new(RefCell::new(0_u32));
    ///         scope
    ///             .spawn_local(async move {
    ///                 // `!Send` state held across an await point.
    ///                 *component.borrow_mut() += 1;
    ///                 tokio::task::yield_now().await;
    ///                 *component.borrow_mut() += 1;
    ///             })
    ///             .unwrap();
    ///     })
    ///     .await;
    /// # }
    /// ```
    pub fn spawn_local<T>(
        &self,
        future: impl Future<Output = T> + 'static,
    ) -> Result<JoinHandle<T>, SpawnError>
    where
        T: 'static,
    {
        self.check_alive()?;
        let join = tokio::task::spawn_local(future);
        self.track(join.abort_handle());
        Ok(join)
    }

    /// Rejects spawning once the lifecycle is destroyed.
    fn check_alive(&self) -> Result<(), SpawnError> {
        if self.is_destroyed() {
            return Err(SpawnError::Destroyed);
        }
        Ok(())
    }

    /// Tracks a task's abort handle for destroy/drop cancellation.
    ///
    /// Finished tasks are pruned so a long-lived scope does not grow without
    /// bound; aborting a finished handle is a no-op.
    fn track(&self, handle: AbortHandle) {
        let mut tasks = self.inner.tasks.borrow_mut();
        tasks.retain(|known| !known.is_finished());
        tasks.push(handle);
    }
}

impl Drop for LifecycleScope {
    fn drop(&mut self) {
        let handles = std::mem::take(&mut *self.inner.tasks.borrow_mut());
        for handle in handles {
            handle.abort();
        }
        // `_subscription` drops after this, removing the destroy observer.
    }
}
