//! Integration tests for [`essenty_lifecycle_tokio::LifecycleScope`].
//!
//! All tests run on the current-thread Tokio runtime so the single-threaded
//! lifecycle can be driven from the same thread that owns the scope.

use essenty_lifecycle::LifecycleRegistry;
use essenty_lifecycle_tokio::{LifecycleScope, SpawnError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn spawn_while_alive_runs_then_destroy_cancels() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let scope = LifecycleScope::new(lifecycle.clone());

    let (started_tx, started_rx) = oneshot::channel::<()>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_probe = Arc::clone(&cancelled);
    let join = scope
        .spawn(async move {
            let _ = started_tx.send(());
            // Run until aborted; `Drop` observation happens via `cancelled`.
            let _guard = CancelFlag(cancelled_probe);
            std::future::pending::<()>().await;
        })
        .unwrap();

    // Task starts while alive.
    started_rx.await.unwrap();
    assert!(!join.is_finished());

    lifecycle.destroy().unwrap();
    // Destroy aborts the owned task; awaiting observes cancellation.
    assert!(join.await.unwrap_err().is_cancelled());
    assert!(cancelled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn multiple_tasks_all_cancelled_on_destroy() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let scope = LifecycleScope::new(lifecycle.clone());

    let (done_tx, mut done_rx) = mpsc::unbounded_channel::<u32>();
    let mut joins = Vec::new();
    for id in 0..4_u32 {
        let tx = done_tx.clone();
        joins.push(
            scope
                .spawn(async move {
                    // Signal liveness, then wait for abort.
                    let _ = tx.send(id);
                    std::future::pending::<()>().await;
                })
                .unwrap(),
        );
    }
    drop(done_tx);

    let mut seen = Vec::new();
    while let Some(id) = done_rx.recv().await {
        seen.push(id);
        if seen.len() == 4 {
            break;
        }
    }
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2, 3]);

    lifecycle.destroy().unwrap();
    for join in joins {
        assert!(join.await.unwrap_err().is_cancelled());
    }
}

#[tokio::test]
async fn drop_scope_cancels_tasks_and_removes_subscription() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let before = lifecycle.subscriber_count();

    let (started_tx, started_rx) = oneshot::channel::<()>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_probe = Arc::clone(&cancelled);
    let join = {
        let scope = LifecycleScope::new(lifecycle.clone());
        assert_eq!(lifecycle.subscriber_count(), before + 1);
        let join = scope
            .spawn(async move {
                let _ = started_tx.send(());
                let _guard = CancelFlag(cancelled_probe);
                std::future::pending::<()>().await;
            })
            .unwrap();
        started_rx.await.unwrap();
        join
    };
    // Scope dropped: subscription removed, task aborted.
    assert_eq!(lifecycle.subscriber_count(), before);
    assert!(join.await.unwrap_err().is_cancelled());
    assert!(cancelled.load(Ordering::SeqCst));

    // Lifecycle itself still works after the scope is gone.
    lifecycle.start().unwrap();
    assert_eq!(lifecycle.state(), essenty_lifecycle::LifecycleState::Started);
}

#[tokio::test]
async fn spawn_after_destroy_is_rejected() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    lifecycle.destroy().unwrap();

    let scope = LifecycleScope::new(lifecycle.clone());
    assert!(scope.is_destroyed());

    let ran = Arc::new(AtomicBool::new(false));
    let probe = Arc::clone(&ran);
    let result = scope.spawn(async move {
        probe.store(true, Ordering::SeqCst);
    });
    assert_eq!(result.unwrap_err(), SpawnError::Destroyed);

    // Let the runtime drain; nothing must have run.
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert!(!ran.load(Ordering::SeqCst));
}

#[tokio::test]
async fn scope_with_explicit_handle_uses_that_runtime() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    let handle = tokio::runtime::Handle::current();
    let scope = LifecycleScope::with_handle(lifecycle.clone(), handle);

    let (tx, rx) = oneshot::channel::<u32>();
    scope
        .spawn(async move {
            let _ = tx.send(41);
        })
        .unwrap();
    assert_eq!(rx.await.unwrap(), 41);
}

struct CancelFlag(Arc<AtomicBool>);

impl Drop for CancelFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
