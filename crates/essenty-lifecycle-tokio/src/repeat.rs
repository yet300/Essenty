use crate::RepeatError;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use std::future::Future;
use tokio::task::JoinHandle;

/// Returns `true` when `state` counts as active for `min_state`.
///
/// `DESTROYED` is terminal and never active, even though its rank compares
/// above every other state.
fn is_active(state: LifecycleState, min_state: LifecycleState) -> bool {
    !state.is_destroyed() && state >= min_state
}

/// Aborts `handle` (if any) and awaits its termination, then clears the slot.
///
/// Awaiting after `abort()` is what prevents two instances of the repeat block
/// from overlapping across rapid transitions: the next child never starts
/// until the previous one has fully terminated.
async fn stop_child(child: &mut Option<JoinHandle<()>>) {
    if let Some(handle) = child.take() {
        handle.abort();
        let _ = handle.await;
    }
}

/// Guard that aborts the child task if the repeat future is dropped
/// (external cancellation) before it could be awaited.
///
/// Dropping a `JoinHandle` detaches the task; without this guard an externally
/// cancelled repeat would leak a running child and its lifecycle subscription
/// cleanup would be incomplete.
struct AbortOnDrop(Option<JoinHandle<()>>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

/// Runs `block` whenever the lifecycle is at least `min_state`, restarting it
/// on every re-entry, until the lifecycle is destroyed.
///
/// This is the Rust equivalent of upstream
/// `Lifecycle.repeatOnLifecycle`: the factory is invoked anew on each entry
/// (futures are never paused/resumed), the running child is aborted when the
/// lifecycle falls below `min_state`, and destruction ends repetition
/// permanently.
///
/// The returned future is `!Send` (it owns the single-threaded lifecycle
/// handle and its subscription) and must be polled on the lifecycle-owning
/// thread — for example by awaiting it directly on a current-thread runtime
/// or inside a `LocalSet`. For thread-local (`!Send`) block futures use
/// [`repeat_on_lifecycle_local`] instead; the two are separate explicit APIs
/// sharing one internal state machine.
///
/// Child block futures are `Send + 'static` because Tokio workers execute
/// them via [`tokio::spawn`]; the factory itself may capture `!Send` data.
///
/// Every lifecycle subscription is removed when the future completes, is
/// cancelled externally, or setup fails.
///
/// # Errors
///
/// Returns [`RepeatError::InvalidMinState`] if `min_state` is
/// [`LifecycleState::Initialized`]. A lifecycle that is already destroyed
/// resolves to `Ok(())` immediately without starting work.
///
/// # Panics
///
/// Panics if no Tokio runtime is active when the first child is spawned
/// (propagates [`tokio::spawn`]'s panic). Call inside a Tokio runtime.
///
/// # Example
///
/// ```rust,no_run
/// use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
/// use essenty_lifecycle_tokio::repeat_on_lifecycle;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let lifecycle = LifecycleRegistry::new();
/// // Await on the owning thread; drive the lifecycle from local tasks.
/// repeat_on_lifecycle(lifecycle, LifecycleState::Started, || async {
///     println!("active");
/// })
/// .await
/// .unwrap();
/// # }
/// ```
pub async fn repeat_on_lifecycle<F, Fut>(
    lifecycle: LifecycleRegistry,
    min_state: LifecycleState,
    block: F,
) -> Result<(), RepeatError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    run_repeat(lifecycle, min_state, block, tokio::spawn).await
}

/// Runs `block` like [`repeat_on_lifecycle`], but the block future may be
/// thread-local (`!Send`).
///
/// This is the route for component work that holds `Rc`, `RefCell`, or other
/// `!Send` state across await points. Child futures require only
/// `Future<Output = ()> + 'static` and run on the thread driving the ambient
/// [`LocalSet`](tokio::task::LocalSet) via [`tokio::task::spawn_local`];
/// restart, abort-then-await, no-overlap, and destroy semantics match
/// [`repeat_on_lifecycle`] exactly because both share the same state machine.
///
/// Call and poll this inside `LocalSet::run_until` or a
/// [`tokio::runtime::LocalRuntime`]. Nothing here creates a `LocalSet`.
///
/// Every lifecycle subscription is removed when the future completes, is
/// cancelled externally, or setup fails.
///
/// # Errors
///
/// Returns [`RepeatError::InvalidMinState`] if `min_state` is
/// [`LifecycleState::Initialized`]. A lifecycle that is already destroyed
/// resolves to `Ok(())` immediately without starting work.
///
/// # Panics
///
/// Panics if a child is launched outside a [`LocalSet`](tokio::task::LocalSet)
/// or local runtime (propagates [`tokio::task::spawn_local`]'s panic). Tokio
/// offers no non-panicking probe for the local-task context, so no typed
/// error is possible here; the contract matches `spawn_local` itself.
///
/// # Example
///
/// ```rust,no_run
/// use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
/// use essenty_lifecycle_tokio::repeat_on_lifecycle_local;
/// use std::cell::RefCell;
/// use std::rc::Rc;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let lifecycle = LifecycleRegistry::new();
/// let local = tokio::task::LocalSet::new();
/// local
///     .run_until(async {
///         let component = Rc::new(RefCell::new(Vec::new()));
///         repeat_on_lifecycle_local(lifecycle, LifecycleState::Started, || {
///             let component = Rc::clone(&component);
///             async move {
///                 // `!Send` state held across an await point.
///                 component.borrow_mut().push(1_u32);
///                 tokio::task::yield_now().await;
///                 component.borrow_mut().push(2_u32);
///             }
///         })
///         .await
///         .unwrap();
///     })
///     .await;
/// # }
/// ```
pub async fn repeat_on_lifecycle_local<F, Fut>(
    lifecycle: LifecycleRegistry,
    min_state: LifecycleState,
    block: F,
) -> Result<(), RepeatError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()> + 'static,
{
    run_repeat(lifecycle, min_state, block, tokio::task::spawn_local).await
}

/// Shared repeat state machine behind [`repeat_on_lifecycle`] and
/// [`repeat_on_lifecycle_local`].
///
/// The only difference between the two public APIs is how a child is
/// launched, so the strategy travels as a plain function pointer —
/// [`tokio::spawn`] for `Send` children, [`tokio::task::spawn_local`] for
/// thread-local ones. No executor trait is introduced; the signatures stay
/// separate and explicit at the public boundary.
async fn run_repeat<F, Fut>(
    lifecycle: LifecycleRegistry,
    min_state: LifecycleState,
    mut block: F,
    spawn_child: fn(Fut) -> JoinHandle<()>,
) -> Result<(), RepeatError>
where
    F: FnMut() -> Fut,
{
    if min_state == LifecycleState::Initialized {
        return Err(RepeatError::InvalidMinState { min_state });
    }
    if lifecycle.state().is_destroyed() {
        return Ok(());
    }

    // `mpsc` (not `watch`) preserves every transition, so a rapid
    // `START → STOP → START` burst cannot coalesce away the intermediate stop
    // and incorrectly keep the first child alive.
    let (events, mut inbox) = tokio::sync::mpsc::unbounded_channel::<LifecycleState>();
    let _subscription = lifecycle.subscribe(move |state| {
        let _ = events.send(state);
    });
    if lifecycle.state().is_destroyed() {
        return Ok(());
    }

    // Subscription replay (`Created`/`Started`/`Resumed` for an already-active
    // lifecycle) is queued in `inbox` and drives the first launch through the
    // normal event loop below. No separate initial spawn is needed — and one
    // would double-start, because the replayed `Started` would launch again.
    let mut child = AbortOnDrop(None);

    while let Some(state) = inbox.recv().await {
        if state.is_destroyed() {
            stop_child(&mut child.0).await;
            return Ok(());
        }
        if is_active(state, min_state) {
            if child.0.is_none() {
                child.0 = Some(spawn_child(block()));
            }
        } else {
            stop_child(&mut child.0).await;
        }
    }

    // The event sender only drops when the subscription observer is cleared
    // (destroy) or the guard drops; reaching here without `DESTROYED` means the
    // channel closed unexpectedly, so stop any child before returning.
    stop_child(&mut child.0).await;
    Ok(())
}
