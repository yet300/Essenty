//! Integration tests for thread-local lifecycle async work:
//! [`LifecycleScope::spawn_local`] and
//! [`essenty_lifecycle_tokio::repeat_on_lifecycle_local`].
//!
//! Every test drives a [`tokio::task::LocalSet`] on the current-thread Tokio
//! runtime; the lifecycle, the scope, and all tasks share that one thread.
//! Thread-local component state (`Rc<RefCell<..>>`) is held *across await
//! points* inside spawned children, which makes those futures `!Send` — the
//! `Send`-bound [`LifecycleScope::spawn`](essenty_lifecycle_tokio::LifecycleScope::spawn)
//! API could not accept them. Synchronization uses deterministic counters,
//! never sleeps.

use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use essenty_lifecycle_tokio::{
    LifecycleScope, RepeatError, ScopeError, SpawnError, repeat_on_lifecycle_local,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// Waits until `cond()` is true, yielding so local tasks can progress.
/// Fails after a bounded number of yields instead of sleeping.
async fn wait_for(cond: impl Fn() -> bool) {
    for _ in 0..1_000 {
        if cond() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("condition not met after yields");
}

#[tokio::test]
async fn try_new_succeeds_inside_runtime() {
    let lifecycle = LifecycleRegistry::new();
    let scope = LifecycleScope::try_new(lifecycle.clone()).unwrap();
    assert!(!scope.is_destroyed());
}

#[test]
fn try_new_outside_runtime_returns_no_runtime() {
    let lifecycle = LifecycleRegistry::new();
    let result = LifecycleScope::try_new(lifecycle.clone());
    assert!(matches!(result.unwrap_err(), ScopeError::NoRuntime(_)));
}

#[tokio::test]
async fn spawn_local_runs_nonsend_state_then_destroy_cancels() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let scope = LifecycleScope::new(lifecycle.clone());
            // Thread-local component state shared with the child.
            let log = Rc::new(RefCell::new(Vec::new()));
            let probe = Rc::clone(&log);
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancelled_probe = Arc::clone(&cancelled);
            let join = scope
                .spawn_local(async move {
                    // `Rc<RefCell>` held across an await point: this future is
                    // `!Send` and only `spawn_local` accepts it.
                    probe.borrow_mut().push(1_u32);
                    tokio::task::yield_now().await;
                    probe.borrow_mut().push(2_u32);
                    let _guard = CancelFlag(cancelled_probe);
                    std::future::pending::<()>().await;
                })
                .unwrap();

            wait_for(|| log.borrow().len() == 2).await;
            assert_eq!(*log.borrow(), vec![1_u32, 2_u32]);

            lifecycle.destroy().unwrap();
            assert!(join.await.unwrap_err().is_cancelled());
            assert!(cancelled.load(Ordering::SeqCst));
        })
        .await;
}

#[tokio::test]
async fn drop_scope_cancels_local_task_and_removes_subscription() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let before = lifecycle.subscriber_count();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let state = Rc::new(RefCell::new(0_u32));
            let probe = Rc::clone(&state);
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancelled_probe = Arc::clone(&cancelled);
            let join = {
                let scope = LifecycleScope::new(lifecycle.clone());
                assert_eq!(lifecycle.subscriber_count(), before + 1);
                let join = scope
                    .spawn_local(async move {
                        *probe.borrow_mut() += 1;
                        tokio::task::yield_now().await;
                        *probe.borrow_mut() += 1;
                        let _guard = CancelFlag(cancelled_probe);
                        std::future::pending::<()>().await;
                    })
                    .unwrap();
                wait_for(|| *state.borrow() == 2).await;
                join
            };
            // Scope dropped: subscription removed, local task aborted.
            assert_eq!(lifecycle.subscriber_count(), before);
            assert!(join.await.unwrap_err().is_cancelled());
            assert!(cancelled.load(Ordering::SeqCst));
            assert_eq!(*state.borrow(), 2);
        })
        .await;

    // Lifecycle itself still works after the scope is gone.
    lifecycle.start().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Started);
}

#[tokio::test]
async fn spawn_local_after_destroy_is_rejected() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    lifecycle.destroy().unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let scope = LifecycleScope::new(lifecycle.clone());
            let ran = Rc::new(RefCell::new(false));
            let probe = Rc::clone(&ran);
            let result = scope.spawn_local(async move {
                *probe.borrow_mut() = true;
            });
            assert_eq!(result.unwrap_err(), SpawnError::Destroyed);
            tokio::task::yield_now().await;
            tokio::task::yield_now().await;
            assert!(!*ran.borrow());
        })
        .await;
}

#[tokio::test]
async fn repeat_local_full_cycle_with_nonsend_blocks() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            // `Rc`-based launch log: each block launch appends its id, then a
            // marker after an await point, proving `!Send` execution.
            let log = Rc::new(RefCell::new(Vec::new()));
            let launches = Arc::new(AtomicUsize::new(0));
            let entered = Arc::new(AtomicUsize::new(0));
            let exited = Arc::new(AtomicUsize::new(0));
            let driver = lifecycle.clone();
            let launches_f = Arc::clone(&launches);
            let log_f = Rc::clone(&log);
            let entered_f = Arc::clone(&entered);
            let exited_f = Arc::clone(&exited);
            let repeater = async {
                let mut id = 0_u32;
                repeat_on_lifecycle_local(driver, LifecycleState::Started, move || {
                    id += 1;
                    let launch = id;
                    launches_f.fetch_add(1, Ordering::SeqCst);
                    let log = Rc::clone(&log_f);
                    let entered = Arc::clone(&entered_f);
                    let exited = Arc::clone(&exited_f);
                    async move {
                        log.borrow_mut().push(launch);
                        tokio::task::yield_now().await;
                        log.borrow_mut().push(launch * 10);
                        entered.fetch_add(1, Ordering::SeqCst);
                        let _guard = ExitFlag(Arc::clone(&exited));
                        std::future::pending::<()>().await;
                    }
                })
                .await
                .unwrap();
            };
            tokio::pin!(repeater);

            () = tokio::select! {
                () = &mut repeater => panic!("repeat completed before destroy"),
                () = async {
                    // Below STARTED: nothing runs.
                    for _ in 0..10 {
                        tokio::task::yield_now().await;
                    }
                    assert_eq!(launches.load(Ordering::SeqCst), 0);
                    // STARTED: first future starts and executes `!Send` work.
                    lifecycle.start().unwrap();
                    wait_for(|| entered.load(Ordering::SeqCst) == 1).await;
                    assert_eq!(*log.borrow(), vec![1_u32, 10_u32]);
                    // CREATED: child cancelled.
                    lifecycle.stop().unwrap();
                    wait_for(|| exited.load(Ordering::SeqCst) == 1).await;
                    // STARTED again: a NEW future runs.
                    lifecycle.start().unwrap();
                    wait_for(|| entered.load(Ordering::SeqCst) == 2).await;
                    assert_eq!(*log.borrow(), vec![1_u32, 10_u32, 2_u32, 20_u32]);
                    // DESTROYED: terminates the repeat permanently.
                    lifecycle.destroy().unwrap();
                } => {},
            };
            repeater.await;
            assert_eq!(launches.load(Ordering::SeqCst), 2);
            assert_eq!(entered.load(Ordering::SeqCst), 2);
            assert_eq!(exited.load(Ordering::SeqCst), 2);
        })
        .await;
}

#[tokio::test]
async fn repeat_local_rejects_initialized_min_state() {
    let lifecycle = LifecycleRegistry::new();
    let before = lifecycle.subscriber_count();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let result = repeat_on_lifecycle_local(
                lifecycle.clone(),
                LifecycleState::Initialized,
                || async {},
            )
            .await;
            assert_eq!(
                result.unwrap_err(),
                RepeatError::InvalidMinState { min_state: LifecycleState::Initialized }
            );
        })
        .await;
    assert_eq!(lifecycle.subscriber_count(), before);
}

#[tokio::test]
async fn rapid_local_transitions_do_not_overlap() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let current = Arc::new(AtomicUsize::new(0));
            let max = Arc::new(AtomicUsize::new(0));
            let entered = Arc::new(AtomicUsize::new(0));
            let driver = lifecycle.clone();
            let current_probe = Arc::clone(&current);
            let max_probe = Arc::clone(&max);
            let entered_probe = Arc::clone(&entered);
            // `Rc` captured by the factory proves the local flavor accepts
            // `!Send` environments even when the child only uses atomics.
            let marker = Rc::new(RefCell::new(0_u32));
            let repeater = async {
                repeat_on_lifecycle_local(driver, LifecycleState::Started, move || {
                    let current = Arc::clone(&current_probe);
                    let max = Arc::clone(&max_probe);
                    let entered = Arc::clone(&entered_probe);
                    let marker = Rc::clone(&marker);
                    async move {
                        *marker.borrow_mut() += 1;
                        entered.fetch_add(1, Ordering::SeqCst);
                        // RAII liveness: abort drops the future mid-execution,
                        // so an explicit decrement after the yields would leak.
                        let _live = LiveGuard(Arc::clone(&current));
                        let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                        max.fetch_max(now, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        tokio::task::yield_now().await;
                        std::future::pending::<()>().await;
                    }
                })
                .await
                .unwrap();
            };
            tokio::pin!(repeater);

            () = tokio::select! {
                () = &mut repeater => panic!("completed early"),
                () = async {
                    for _ in 0..5 {
                        lifecycle.start().unwrap();
                        wait_for(|| entered.load(Ordering::SeqCst) >= 1).await;
                        lifecycle.stop().unwrap();
                        tokio::task::yield_now().await;
                    }
                    // A rapid synchronous burst on top: the `mpsc` queue
                    // preserves every transition so the stop cannot coalesce.
                    for _ in 0..10 {
                        lifecycle.start().unwrap();
                        lifecycle.stop().unwrap();
                    }
                    lifecycle.start().unwrap();
                    wait_for(|| entered.load(Ordering::SeqCst) >= 2).await;
                    lifecycle.destroy().unwrap();
                } => {},
            };
            repeater.await;

            let peak = max.load(Ordering::SeqCst);
            assert!(peak <= 1, "local repeat blocks overlapped (peak {peak})");
            assert!(entered.load(Ordering::SeqCst) >= 1, "no local child executed");
            assert_eq!(current.load(Ordering::SeqCst), 0);
        })
        .await;
}

struct CancelFlag(Arc<AtomicBool>);

impl Drop for CancelFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct ExitFlag(Arc<AtomicUsize>);

impl Drop for ExitFlag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct LiveGuard(Arc<AtomicUsize>);

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
