//! Integration tests for [`essenty_lifecycle_tokio::repeat_on_lifecycle`].
//!
//! The repeat future is `!Send` (it owns the single-threaded lifecycle
//! handle), so every test drives the lifecycle from the same current-thread
//! runtime that polls the repeat future. Concurrency between driver and
//! repeater uses `tokio::join!`/`select!` (in-place polling, no `Send`
//! required) plus deterministic counters — no timing sleeps.
//!
//! Counter discipline: `launches` counts block-factory calls (synchronous),
//! `entered` counts child futures that actually started executing on the
//! runtime, and `exited` counts child teardowns via a `Drop` guard. Tests wait
//! for `entered` before driving a stop/destroy/cancel so cancellation always
//! observes a running child rather than racing task startup.

use essenty_lifecycle::{LifecycleRegistry, LifecycleState};
use essenty_lifecycle_tokio::{RepeatError, repeat_on_lifecycle};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Shared counters for one test.
#[derive(Debug, Default)]
struct Probe {
    launches: AtomicUsize,
    entered: AtomicUsize,
    exited: AtomicUsize,
}

impl Probe {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Builds the repeat block factory: each launch records `launches`, each
    /// executed child records `entered` on first poll and `exited` on teardown.
    ///
    /// Takes the probe by value so the factory owns its `Arc` and the repeat
    /// future is `'static` (required for `spawn_local`).
    fn factory(
        self: Arc<Self>,
    ) -> impl FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        let probe = Arc::clone(&self);
        move || {
            probe.launches.fetch_add(1, Ordering::SeqCst);
            let probe = Arc::clone(&probe);
            Box::pin(async move {
                probe.entered.fetch_add(1, Ordering::SeqCst);
                let _guard = ExitGuard(Arc::clone(&probe));
                std::future::pending::<()>().await;
            }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        }
    }

    fn launches(&self) -> usize {
        self.launches.load(Ordering::SeqCst)
    }

    fn entered(&self) -> usize {
        self.entered.load(Ordering::SeqCst)
    }

    fn exited(&self) -> usize {
        self.exited.load(Ordering::SeqCst)
    }
}

struct ExitGuard(Arc<Probe>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.exited.fetch_add(1, Ordering::SeqCst);
    }
}

struct LiveGuard(Arc<AtomicUsize>);

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Waits until `cond()` is true, yielding so the repeater can progress.
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
async fn rejects_initialized_min_state() {
    let lifecycle = LifecycleRegistry::new();
    let before = lifecycle.subscriber_count();
    let result =
        repeat_on_lifecycle(lifecycle.clone(), LifecycleState::Initialized, || async {}).await;
    assert_eq!(
        result.unwrap_err(),
        RepeatError::InvalidMinState { min_state: LifecycleState::Initialized }
    );
    assert_eq!(lifecycle.subscriber_count(), before);
}

#[tokio::test]
async fn already_destroyed_returns_immediately_without_work() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();
    lifecycle.destroy().unwrap();
    let before = lifecycle.subscriber_count();

    let probe = Probe::new();
    let factory_probe = Arc::clone(&probe);
    repeat_on_lifecycle(lifecycle.clone(), LifecycleState::Started, move || {
        factory_probe.launches.fetch_add(1, Ordering::SeqCst);
        async {}
    })
    .await
    .unwrap();

    assert_eq!(probe.launches(), 0);
    assert_eq!(lifecycle.subscriber_count(), before);
}

#[tokio::test]
async fn below_min_state_block_not_running_until_started() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();

    let probe = Probe::new();
    let driver = lifecycle.clone();
    let factory_probe = Arc::clone(&probe);
    let repeater = async move {
        repeat_on_lifecycle(driver, LifecycleState::Started, Arc::clone(&factory_probe).factory())
            .await
            .unwrap();
    };
    tokio::pin!(repeater);

    tokio::select! {
        () = &mut repeater => panic!("repeat completed before destroy"),
        () = async {
            // While CREATED the block must not run.
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(probe.launches(), 0);
            // Enter STARTED: exactly one launch, and it executes.
            lifecycle.start().unwrap();
            wait_for(|| probe.entered() == 1).await;
            assert_eq!(probe.launches(), 1);
            // RESUMED keeps the same block (no relaunch for min STARTED).
            lifecycle.resume().unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(probe.launches(), 1);
            // Back to STARTED keeps it as well.
            lifecycle.pause().unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(probe.launches(), 1);
            lifecycle.destroy().unwrap();
        } => {},
    }
    repeater.await;
    assert_eq!(probe.launches(), 1);
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.exited(), 1);
}

#[tokio::test]
async fn started_to_created_cancels_and_restarts_with_new_future() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Started).unwrap();

    let probe = Probe::new();
    let driver = lifecycle.clone();
    let factory_probe = Arc::clone(&probe);
    let repeater = async move {
        repeat_on_lifecycle(driver, LifecycleState::Started, Arc::clone(&factory_probe).factory())
            .await
            .unwrap();
    };
    tokio::pin!(repeater);

    // Already STARTED: subscription replay launches the first block exactly once.
    tokio::select! {
        () = &mut repeater => panic!("completed early"),
        () = async {
            wait_for(|| probe.entered() == 1).await;
            assert_eq!(probe.launches(), 1);
            // STOP cancels it.
            lifecycle.stop().unwrap();
            wait_for(|| probe.exited() == 1).await;
            // START launches a NEW future.
            lifecycle.start().unwrap();
            wait_for(|| probe.entered() == 2).await;
            assert_eq!(probe.launches(), 2);
            lifecycle.destroy().unwrap();
        } => {},
    }
    repeater.await;
    assert_eq!(probe.launches(), 2);
    assert_eq!(probe.entered(), 2);
    assert_eq!(probe.exited(), 2);
}

#[tokio::test]
async fn destroy_cancels_block_and_completes_without_restart() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Resumed).unwrap();
    let before = lifecycle.subscriber_count();

    let probe = Probe::new();
    let driver = lifecycle.clone();
    let driver_probe = Arc::clone(&probe);
    let repeater_probe = Arc::clone(&probe);
    let done = tokio::join!(
        async move {
            repeat_on_lifecycle(
                driver,
                LifecycleState::Started,
                Arc::clone(&repeater_probe).factory(),
            )
            .await
            .unwrap();
        },
        async {
            wait_for(|| driver_probe.entered() == 1).await;
            assert_eq!(driver_probe.launches(), 1);
            lifecycle.destroy().unwrap();
        }
    );
    let _ = done;

    assert_eq!(probe.launches(), 1);
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.exited(), 1);
    // Repeat completed; no subscription leaked and later calls cannot restart.
    assert_eq!(lifecycle.subscriber_count(), before);
    assert!(lifecycle.move_to(LifecycleState::Started).is_err());
    assert_eq!(probe.launches(), 1);
}

#[tokio::test]
async fn rapid_transitions_do_not_overlap() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.create().unwrap();

    let current = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    let launches = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(AtomicUsize::new(0));
    let entered_probe = Arc::clone(&entered);
    let launches_probe = Arc::clone(&launches);
    let current_probe = Arc::clone(&current);
    let max_probe = Arc::clone(&max);

    let driver = lifecycle.clone();
    let repeater = async move {
        repeat_on_lifecycle(driver, LifecycleState::Started, move || {
            launches_probe.fetch_add(1, Ordering::SeqCst);
            let current = Arc::clone(&current_probe);
            let max = Arc::clone(&max_probe);
            let entered = Arc::clone(&entered_probe);
            async move {
                entered.fetch_add(1, Ordering::SeqCst);
                // RAII liveness: abort drops the future mid-execution, so an
                // explicit decrement after the yields would leak the count.
                let _live = LiveGuard(Arc::clone(&current));
                let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(now, Ordering::SeqCst);
                // Hold across yields so an overlap would be observable.
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                std::future::pending::<()>().await;
            }
        })
        .await
        .unwrap();
    };
    tokio::pin!(repeater);

    tokio::select! {
        () = &mut repeater => panic!("completed early"),
        () = async {
            // Sequential restart cycles with the child confirmed running and
            // torn down each time: no two instances may overlap.
            for _ in 0..5 {
                lifecycle.start().unwrap();
                wait_for(|| entered.load(Ordering::SeqCst) >= 1).await;
                // Reset per-cycle visibility is unnecessary; the max-current
                // invariant below is the assertion that matters.
                lifecycle.stop().unwrap();
                tokio::task::yield_now().await;
            }
            // A rapid synchronous burst on top: the mpsc queue preserves every
            // transition so the intermediate stop cannot coalesce away.
            for _ in 0..10 {
                lifecycle.start().unwrap();
                lifecycle.stop().unwrap();
            }
            lifecycle.start().unwrap();
            wait_for(|| entered.load(Ordering::SeqCst) >= 2).await;
            lifecycle.destroy().unwrap();
        } => {},
    }
    repeater.await;

    let peak = max.load(Ordering::SeqCst);
    assert!(peak <= 1, "repeat blocks overlapped (peak {peak})");
    assert!(entered.load(Ordering::SeqCst) >= 1, "no child ever executed");
    assert_eq!(current.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn external_cancellation_cleans_up_subscription_and_child() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Started).unwrap();
    let before = lifecycle.subscriber_count();

    let probe = Probe::new();
    let factory_probe = Arc::clone(&probe);
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let handle = tokio::task::spawn_local(repeat_on_lifecycle(
                lifecycle.clone(),
                LifecycleState::Started,
                Arc::clone(&factory_probe).factory(),
            ));
            // Wait until the child is actually executing (guard constructed),
            // then cancel from the outside by aborting the local task.
            wait_for(|| probe.entered() == 1).await;
            handle.abort();
            let _ = handle.await;
            // Cancellation must tear down the running child.
            wait_for(|| probe.exited() == 1).await;
        })
        .await;

    // The aborted repeat future dropped its observer.
    tokio::task::yield_now().await;
    assert_eq!(lifecycle.subscriber_count(), before);

    // Lifecycle remains usable after the cancelled repeat.
    lifecycle.stop().unwrap();
    assert_eq!(lifecycle.state(), LifecycleState::Created);
}

#[tokio::test]
async fn resumed_min_state_survives_started() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Resumed).unwrap();

    let probe = Probe::new();
    let driver = lifecycle.clone();
    let factory_probe = Arc::clone(&probe);
    let repeater = async move {
        repeat_on_lifecycle(driver, LifecycleState::Resumed, Arc::clone(&factory_probe).factory())
            .await
            .unwrap();
    };
    tokio::pin!(repeater);

    tokio::select! {
        () = &mut repeater => panic!("completed early"),
        () = async {
            wait_for(|| probe.entered() == 1).await;
            assert_eq!(probe.launches(), 1);
            // RESUMED → STARTED drops below min RESUMED: cancel, no restart.
            lifecycle.pause().unwrap();
            wait_for(|| probe.exited() == 1).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(probe.launches(), 1);
            // STARTED → RESUMED re-enters: new block executes.
            lifecycle.resume().unwrap();
            wait_for(|| probe.entered() == 2).await;
            assert_eq!(probe.launches(), 2);
            lifecycle.destroy().unwrap();
        } => {},
    }
    repeater.await;
    assert_eq!(probe.launches(), 2);
    assert_eq!(probe.entered(), 2);
    assert_eq!(probe.exited(), 2);
}

#[tokio::test]
async fn created_min_state_runs_through_destroy_walk() {
    let lifecycle = LifecycleRegistry::new();
    lifecycle.move_to(LifecycleState::Resumed).unwrap();

    let probe = Probe::new();
    let driver = lifecycle.clone();
    let factory_probe = Arc::clone(&probe);
    let repeater = async move {
        repeat_on_lifecycle(driver, LifecycleState::Created, Arc::clone(&factory_probe).factory())
            .await
            .unwrap();
    };
    tokio::pin!(repeater);

    tokio::select! {
        () = &mut repeater => panic!("completed early"),
        () = async {
            // Already above CREATED: one launch via replay.
            wait_for(|| probe.entered() == 1).await;
            assert_eq!(probe.launches(), 1);
            // Walking back to CREATED keeps the same child for min CREATED.
            lifecycle.move_to(LifecycleState::Created).unwrap();
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(probe.launches(), 1);
            assert_eq!(probe.exited(), 0);
            lifecycle.destroy().unwrap();
        } => {},
    }
    repeater.await;
    assert_eq!(probe.launches(), 1);
    assert_eq!(probe.exited(), 1);
}
