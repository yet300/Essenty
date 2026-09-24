use crate::SpawnError;
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
/// must live on the lifecycle-owning thread. Spawned futures are stricter
/// (`Send + 'static`) because Tokio workers execute them; that bound stays
/// local to [`LifecycleScope::spawn`].
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
    /// If `lifecycle` is already destroyed, the scope is created inert:
    /// every [`spawn`](Self::spawn) returns [`SpawnError::Destroyed`].
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime (propagates
    /// [`Handle::current`]'s panic). Prefer [`with_handle`](Self::with_handle)
    /// in libraries and tests.
    #[must_use]
    pub fn new(lifecycle: LifecycleRegistry) -> Self {
        Self::with_handle(lifecycle, Handle::current())
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
        if self.is_destroyed() {
            return Err(SpawnError::Destroyed);
        }
        // Prune finished tasks so a long-lived scope does not grow without
        // bound; finished aborts are no-ops.
        let mut tasks = self.inner.tasks.borrow_mut();
        tasks.retain(|handle| !handle.is_finished());
        let join = self.inner.handle.spawn(future);
        tasks.push(join.abort_handle());
        Ok(join)
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
