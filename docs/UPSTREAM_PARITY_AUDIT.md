# Upstream parity audit

```text
Upstream repository:
https://github.com/arkivanov/Essenty

Upstream commit:
c4f1e914185daa21de4867716a102b44b9a945a3

Rust repository commit before audit:
620921506ef86975994d49092a5d78dff33868d7
```

Audit branch: `codex/full-essenty-parity-audit`. All compatibility claims below
reference the pinned upstream commit (production source and tests, not README).
Status vocabulary: `MATCHES`, `SEMANTICALLY_EQUIVALENT`, `INTENTIONALLY_DIFFERENT`,
`MISSING`, `PARTIAL`, `NOT_APPLICABLE`, `RUST_EXTENSION`, `UNVERIFIED`.

Platform coverage (per-platform tables, runtime taxonomy, Matrix A vs. Matrix B)
lives in [`PLATFORM_PARITY.md`](PLATFORM_PARITY.md). Public API review lives in
[`PUBLIC_API_AUDIT.md`](PUBLIC_API_AUDIT.md). Decompose readiness lives in
[`DECOMPOSE_READINESS.md`](DECOMPOSE_READINESS.md).

## 1. Upstream modules discovered (pinned commit)

`settings.gradle.kts` includes: `utils-internal`, `lifecycle`,
`lifecycle-coroutines`, `lifecycle-reaktive`, `state-keeper`,
`state-keeper-benchmarks`, `instance-keeper`, `back-handler`
(`tools:check-publication` only in publication-check mode).

| Upstream module | Kind | Rust counterpart |
|---|---|---|
| `lifecycle` (common + android + itvos) | Core primitive | `essenty-lifecycle` + `essenty-android`/`essenty-apple` adapters |
| `lifecycle-coroutines` (common only) | Kotlin async integration | None (assessed, not ported) |
| `lifecycle-reaktive` (common only) | Kotlin reactive integration | None (assessed, not ported) |
| `state-keeper` (common + java/nonJava + android) | Core primitive | `essenty-state-keeper` + `essenty-android` envelope |
| `instance-keeper` (common + android) | Core primitive | `essenty-instance-keeper` (+ documented host contract) |
| `back-handler` (common + android) | Core primitive | `essenty-back-handler` + `essenty-android` platform adapter |
| `utils-internal` (`InternalEssentyApi`, `ExperimentalEssentyApi`) | Internal markers | `RUST_EXTENSION`-free: no equivalent needed (see §9) |
| `state-keeper-benchmarks` | Benchmarks only | `docs/BENCHMARK_PLAN.md` (plan, no perf claims) |

## 2. Upstream platform targets (from `build.gradle.kts`)

```text
androidTarget(), jvm(), js { browser(); nodejs() }, wasmJs { browser() },
linuxX64(), iosCompat(), watchosCompat(), tvosCompat(), macosCompat()
AndroidConfig(minSdkVersion = 15, compileSdkVersion = 34, targetSdkVersion = 34)
```

Verified per-module source sets at the pinned commit:

- `lifecycle`: `commonMain`, `androidMain` (AndroidX interop only), `itvosMain`
  (`ApplicationLifecycle`, shared iOS/tvOS). No watchOS/macOS/JVM/JS-specific
  lifecycle integration.
- `state-keeper`: `commonMain`, `javaMain`/`nonJavaMain` (serialization
  expect/actual), `androidMain` (SavedStateRegistry interop). No JS- or
  Apple-specific integration.
- `instance-keeper`: `commonMain`, `androidMain` (ViewModelStore interop).
- `back-handler`: `commonMain`, `androidMain` (OnBackPressedDispatcher interop).
- `lifecycle-coroutines`, `lifecycle-reaktive`: `commonMain` only, no platform
  source sets.

Core-vs-integration distinction (mandatory): every KMP target compiles the
common core; native OS wiring exists only for Android (all four modules, via
AndroidX) and iOS/tvOS (lifecycle only, via `ApplicationLifecycle`).

## 3. Lifecycle parity (`essenty-lifecycle`)

Upstream refs: `Lifecycle.kt`, `LifecycleRegistry.kt`,
`LifecycleRegistryImpl.kt`, `LifecycleRegistryExt.kt`, `LifecycleExt.kt`,
tests `LifecycleRegistryTest.kt`, `LifecycleExtTest.kt`, API dump
`lifecycle/api/jvm/lifecycle.api`.

| Behavior | Status | Upstream ref | Rust ref |
|---|---|---|---|
| States Initialized/Created/Started/Resumed/Destroyed; strict adjacent forward+reverse graph | MATCHES | `LifecycleRegistryImpl.onCreate/onStart/onResume/onPause/onStop/onDestroy` + `checkState` | `registry.rs` strict step methods; `full_forward_and_backward_cycle`, `invalid_strict_steps_are_rejected` |
| New subscribers replay Created/Started/Resumed for current state | MATCHES | `LifecycleRegistryImpl.subscribe` | `registry.rs::subscribe` replay loop; `late_subscriber_replays_current_state_then_receives_future_events` |
| Forward callbacks in registration order; pause/stop/destroy in reverse order | MATCHES | `LifecycleRegistryTest.WHEN_called_THEN_calls_subscribers_in_correct_order` | `observers_fire_in_subscription_order`, `reverse_events_notify_in_reverse_subscription_order` |
| Unsubscribe self from callback; snapshot delivery for current event | MATCHES | `LifecycleRegistryTest.WHEN_unsubscribed_from_callback...` | `observer_can_unsubscribe_itself`, `removed_observer_still_receives_current_snapshot_event` |
| Destroy from Resumed walks pause/stop/destroy; terminal registry clears observers | MATCHES | `LifecycleRegistryExt.destroy()` + `onDestroy` clears `callbacks` | `destroy()` via `move_to(Created)` + clear; `destroy_notifies_observers_once` |
| Callback destroys registry mid-dispatch | MATCHES | (no explicit test; immutable-set iteration is safe) | `callback_can_destroy_during_create` |
| Tolerant `create/start/resume/pause/stop/destroy` drivers (no-op when already there or destroyed) | INTENTIONALLY_DIFFERENT | `LifecycleRegistryExt.kt` (all tolerant) | Strict step methods return typed errors; idempotent host driving uses `move_to`. Rationale: invalid manual transitions stay observable; adapters use `move_to`. |
| Repeated `destroy()` | INTENTIONALLY_DIFFERENT | ext `destroy()` is a no-op when destroyed | `destroy()` returns `AlreadyDestroyed`; `destroyed_is_terminal`. Rationale: typed terminal error exposes accidental double teardown. |
| Duplicate observer identity rejected | INTENTIONALLY_DIFFERENT | `subscribe` checks `callbacks !in this.callbacks` | Unique token per registration; same closure may register twice. Rationale: tokens make removal unambiguous. |
| `doOnCreate/doOnStart/doOnResume/doOnPause/doOnStop/doOnDestroy` + one-time variants | MATCHES (implemented in this audit) | `LifecycleExt.kt` + `LifecycleExtTest.kt` | `do_on_create/do_on_start[_once]/do_on_resume[_once]/do_on_pause[_once]/do_on_stop[_once]/do_on_destroy`; direction tracked from consecutive deliveries so pause≠start and stop≠create; 8 new regression tests |
| Generic multi-callback `Lifecycle.subscribe(onCreate=..., ...)` convenience | INTENTIONALLY_DIFFERENT | `LifecycleExtKt.subscribe` (see API dump) | Covered by the `do_on_*` family; no combined struct added to avoid API burden. |
| `LifecycleRegistry(initialState)` non-default start | INTENTIONALLY_DIFFERENT | `LifecycleRegistry(initialState)` factory | Registries always start `Initialized`; adapters drive forward with `move_to`. No Decompose use case found. |
| `LifecycleOwner` interface | INTENTIONALLY_DIFFERENT | `LifecycleOwner.kt` | No Owner trait. Rationale: Rust composition (pass `&LifecycleRegistry`) subsumes it; see `DECOMPOSE_READINESS.md`. |
| AndroidX `asEssentyLifecycle` interop | NOT_APPLICABLE | `androidMain/AndroidExt.kt` | Rust has no AndroidX host; `AndroidLifecycle`/`NativeActivityLifecycle` are the native equivalents (see `PLATFORM_PARITY.md`). |

Reentrancy note: nested transitions during dispatch notify synchronously in
both implementations (upstream iterates an immutable set snapshot; Rust
snapshots the observer `Rc` list per event and `move_to` re-reads state after
every callback). Arbitrary nested-transition ordering is covered by
`callback_can_destroy_during_create` and the `do_on_*` direction tests, not by
an exhaustive matrix — no evidence of divergence found.

## 4. lifecycle-coroutines assessment

Upstream (`CoroutineScopeWithLifecycle.kt`, `FlowWithLifecycle.kt`,
`RepeatOnLifecycle.kt`, `DispatchersExt.kt`, 3 test files): lifecycle-bound
`CoroutineScope`, `Flow.withLifecycle`, `repeatOnLifecycle`, Main-dispatcher
helpers. Status: **upstream Kotlin Coroutine API is language-specific; the
lifecycle-aware async capability is implemented through
`essenty-lifecycle-tokio`** (optional integration crate, core stays
Tokio-free).

| Upstream | Rust |
|---|---|
| `CoroutineScopeWithLifecycle` / `coroutineScope()` / `withLifecycle` (cancel on destroy; inactive when created destroyed) | `LifecycleScope`: destroy aborts every owned task, scope drop aborts + unsubscribes, spawn after destroy is `SpawnError::Destroyed` (scope tests) |
| `Lifecycle.repeatOnLifecycle` (reject `INITIALIZED`; immediate return on `DESTROYED`; cancel/relaunch across active states; Mutex against overlap; unsubscribe in `finally`) | `repeat_on_lifecycle` over `CREATED`/`STARTED`/`RESUMED`: `RepeatError::InvalidMinState` for `INITIALIZED`, immediate `Ok` on `DESTROYED`, factory-reconstructed (never paused) child per entry, abort-then-await before restart so rapid `START → STOP → START` never overlaps, RAII unsubscription on completion/external cancellation (repeat tests, incl. rapid + external-cancel + destroy cases); `repeat_on_lifecycle_local` shares the same state machine for `!Send` block futures (local tests with `Rc`/`RefCell` across await) |
| `Flow.withLifecycle` | Deferred: no `Stream` adapter. Upstream it is a thin wrapper over `repeatOnLifecycle`; scope + repeat cover the capability without a new framework. |
| `Dispatchers.Main.immediateOrFallback` | Intentionally absent: Tokio has no UI-main equivalent; platform dispatch belongs to a future UI integration. |

Dependency direction is `essenty-lifecycle-tokio → essenty-lifecycle → core`;
no core crate depends on Tokio (verified via dependency tree). Scope and both
repeat futures are `!Send` (lifecycle thread). `Send` and thread-local work
are separate explicit APIs sharing one internal state machine:
`spawn` / `repeat_on_lifecycle` take `Send + 'static` children run on Tokio
workers, while `spawn_local` / `repeat_on_lifecycle_local` take
`Future + 'static` children run on the owning thread's `LocalSet` (which the
scope never creates; Tokio offers no non-panicking probe for that context, so
the local APIs propagate its panic by contract). Scope construction is
`with_handle` (explicit), `try_new` (typed `ScopeError` outside a runtime),
or the panicking convenience `new`.

## 5. lifecycle-reaktive assessment

Upstream (`DisposableWithLifecycle.kt` + test): `DisposableScope()` /
`Disposable.withLifecycle` disposed on destroy. Status: **Reaktive-specific
API not ported; lifecycle-bound async/resource cleanup is covered by the Tokio
integration + Rust RAII**. `Subscription` (lifecycle), `Drop` (instances),
`do_on_destroy` (arbitrary cleanup), `LifecycleScope` drop/destroy abort, and
`repeat_on_lifecycle` external-cancellation abort + unsubscribe cover every
behavior the Reaktive helpers supply. No reactive framework was introduced.

## 6. StateKeeper parity (`essenty-state-keeper`)

Upstream refs: `StateKeeper.kt`, `StateKeeperDispatcher.kt`,
`DefaultStateKeeperDispatcher.kt`, `StateKeeperExt.kt` (experimental
`saveable`), `SerializableContainer.kt`, `PolymorphicSerializer.kt`,
`ExperimentalStateKeeperApi.kt`, `Utils.kt` (+`javaMain`/`nonJavaMain`
expect/actual), tests `DefaultStateKeeperDispatcherTest.kt`,
`StateKeeperExtTest.kt`, `SerializableContainerTest.kt`,
`PolymorphicSerializerTest.kt`, `CodingTest.kt`, API dump.

| Behavior | Status | Upstream ref | Rust ref |
|---|---|---|---|
| Unique provider keys; duplicate rejected | MATCHES | `register` checks `!isRegistered` | `duplicate_registration_is_rejected` |
| Restored value consumed exactly once | MATCHES | `consume` removes from `savedState` | `consume_is_single_shot` |
| Providers evaluated at save time | MATCHES | `save()` calls suppliers | `bytes_round_trip`, `encode_failure_surfaces_at_save` |
| Unconsumed restored entries survive another save; live providers overwrite same keys | MATCHES | `save()` merges `savedState` + suppliers | `unconsumed_restored_values_survive_another_save` |
| Nullable supplier (`() -> T?`) returning null skips the key, preserving old restored value | MATCHES (implemented in this audit) | `DefaultStateKeeperDispatcherTest` registers `{ null }` supplier | `register_optional`/`register_optional_value`; `save()` skips `None`; 4 new regression tests |
| `isRegistered` query | MATCHES | `StateKeeper.isRegistered` | `is_registered` alias (+ existing `has_provider`) |
| `unregister` semantics | INTENTIONALLY_DIFFERENT | `unregister` checks membership (throws when absent) | Returns `bool`; idempotent cleanup. Unregister keeps restored entry in both. |
| `saveable` delegated-property helpers (experimental) | NOT_APPLICABLE | `StateKeeperExt.kt` | Kotlin delegated properties have no Rust analogue; explicit register/consume is the idiom. |
| `SerializableContainer` lazy holder + base64 `Serializer`, `consumeRequired`, `PolymorphicSerializer`, java/nonJava `serialize`/`deserialize` expect/actual | INTENTIONALLY_DIFFERENT | `SerializableContainer.kt`, `PolymorphicSerializer.kt`, `Utils.kt` | Opaque `Vec<u8>` + caller-chosen serde codec (`register_value`/`consume_value`); format independence is a superset for single-language use. Cross-language binary compatibility is explicitly out of scope. |
| `StateKeeperOwner` | INTENTIONALLY_DIFFERENT | `StateKeeperOwner.kt` | No Owner trait; pass `&mut StateKeeper` (see `DECOMPOSE_READINESS.md`). |
| Deterministic save ordering | INTENTIONALLY_DIFFERENT (stronger) | `HashMap` suppliers, no order promise | `BTreeMap` key-ordered snapshots; `multiple_keys_save_in_order`. |
| Android `StateKeeper(savedStateRegistry, key, discardSavedState, isSavingAllowed)` | SEMANTICALLY_EQUIVALENT within NativeActivity contract | `androidMain/AndroidExt.kt` | `NativeActivityState::from_bytes` (+ envelope): discard ≡ restore from empty bytes; conditional save ≡ caller-gated `save_bytes`; custom key ≡ key namespacing (single opaque map). Malformed state → typed `NativeStateError` (stricter than Bundle passthrough). See `PLATFORM_PARITY.md`. |

## 7. InstanceKeeper parity (`essenty-instance-keeper`)

Upstream refs: `InstanceKeeper.kt`, `InstanceKeeperDispatcher.kt`,
`DefaultInstanceKeeperDispatcher.kt`, `InstanceKeeperExt.kt`,
`InstanceKeeperOwner.kt`, `ExperimentalInstanceKeeperApi.kt`, tests
`DefaultInstanceKeeperDispatcherTest.kt`, `InstanceKeeperExtTest.kt`.

| Behavior | Status | Upstream ref | Rust ref |
|---|---|---|---|
| Same key yields same instance; factory runs once | MATCHES | `getOrCreate` + ext test | `get_or_create_returns_same_instance` |
| Missing key → none; multiple keys independent | MATCHES | `get` contract | `get_returns_none_until_created`, `multiple_keys_are_independent` |
| String keys + `TypeMismatch` error | INTENTIONALLY_DIFFERENT | `Any` keys + unchecked casts | String keys + typed `TypeMismatch`; original value intact. Rationale: explicit over panicking casts. |
| Explicit `put` of prebuilt instance; duplicate rejected | MATCHES (implemented in this audit) | `InstanceKeeper.put` (throws on duplicate) | `put` + `DuplicateKey` variant; `put_retains_prebuilt_instance_and_rejects_duplicates`, `put_after_destroy_is_rejected` |
| Explicit `remove` returning the instance without destroying | MATCHES (implemented in this audit) | `remove` (does not destroy) | `remove` returns `Option<Rc<T>>`; mismatch re-inserts + errors; `remove_returns_instance_without_destroying_it`, `remove_absent_key_returns_none_and_mismatch_keeps_value` |
| Scope destruction idempotent | MATCHES | `destroy()` guards `isDestroyed` | `terminal_destroy_is_idempotent_and_rejects_new_instances` |
| Post-destroy `put` immediately destroys vs. rejected | INTENTIONALLY_DIFFERENT | `put` stores then calls `onDestroy` | `put`/`get_or_create` return `Destroyed`. Rationale: retaining an immediately-doomed `Rc` is a footgun; typed error matches the rest of the API. |
| Post-destroy `get` still returns instances vs. cleared | INTENTIONALLY_DIFFERENT | map retained after `destroy()` | `destroy_all` clears + sets flag. Rationale: ended scope holds nothing; Decompose pops are terminal. |
| `onDestroy` at scope end even with live references vs. `Drop` on last `Rc` release | INTENTIONALLY_DIFFERENT | `destroy()` calls `onDestroy` on all values | `Drop` runs after keeper + external clones drop; `value_drops_when_keeper_and_clones_are_gone`. Rationale: Rust ownership; documented pre-existing decision. |
| `getOrCreateCloseable` / `retainingCloseable` | NOT_APPLICABLE (subsumed) | `InstanceKeeperExt.kt` | `Drop` subsumes `Closeable.onDestroy`; no wrapper needed. |
| `getOrCreateSimple` / `retainingSimpleInstance` | SEMANTICALLY_EQUIVALENT | `InstanceKeeperExt.kt` | Plain `get_or_create` over any `T: 'static` already is the simple path (no `Instance` bound exists). |
| `retainingInstance` / `retainedInstance` / `retainedSimpleInstance` delegated properties + `InstanceKeeperOwner` | NOT_APPLICABLE / INTENTIONALLY_DIFFERENT | `InstanceKeeperExt.kt` (experimental delegates), `InstanceKeeperOwner.kt` | No delegated properties or Owner trait in Rust; direct keeper access is the idiom. |
| Deprecated typeless `getOrCreate(factory)` overloads | NOT_APPLICABLE | `InstanceKeeperExt.kt` (`typeOf<T>()` keys, deprecated upstream) | String keys only; the deprecated pattern was never adopted. |
| Android `InstanceKeeper(viewModelStore, discardRetainedInstances)` | SEMANTICALLY_EQUIVALENT within NativeActivity contract | `androidMain/AndroidExt.kt` | Handled `configChanges` keep one `android_main` + one local `Rc` keeper alive (API 36.1 runtime-verified); arbitrary recreation is `UNSUPPORTED for non-Send values` by design; discard ≡ `clear()` + fresh keeper. See `PLATFORM_PARITY.md`. |

## 8. BackHandler parity (`essenty-back-handler`)

Upstream refs: `BackHandler.kt`, `BackHandlerOwner.kt`, `BackDispatcher.kt`,
`BackCallback.kt`, `DefaultBackDispatcher.kt`, `BackEvent.kt`, `Utils.kt`
(`findMostImportant`), test `DefaultBackDispatcherTest.kt` (60+ cases).

| Behavior | Status | Upstream ref | Rust ref |
|---|---|---|---|
| Highest `(priority, registration order)` enabled handler wins; ties → latest | MATCHES | `findMostImportant` (`sortedBy(priority).lastOrNull(isEnabled)`); priority test with 8 callbacks | `highest_priority_wins`, `later_registration_breaks_priority_ties`; note Rust IDs ascend so `max_by_key(priority, id)` ≡ upstream sort-last |
| Disabled handlers skipped | MATCHES | `GIVEN_enabled_callbacks_registered_and_then_some_disabled...` | `disabled_handler_is_skipped` |
| Gesture claimed at start; later registration does not steal it | MATCHES | `WHEN_another_callback_registered_while_in_progress...` | `gesture_sticks_to_claimed_handler` |
| Removing gesture owner cancels it; later progress selects fallback **with the original start event** | MATCHES (fallback replay fixed in this audit) | `GIVEN_two_callbacks_registered_and_gesture_started_and_progress_callback_unregistered_WHEN_progress_THEN_another_callback_started_and_progressed` | `removed_gesture_owner_is_cancelled_and_fallback_starts_on_progress`, `fallback_after_owner_removal_replays_original_start_event` (position-preserving) |
| `back()` during gesture routes to claimed owner; after unregister routes to fallback without restart | MATCHES | `..._WHEN_back_THEN_another_callback_back`, `WHEN_progress_with_back...` | `predictive_gesture_lifecycle`, `gesture_sticks_to_claimed_handler` |
| `cancelPredictiveBack` after owner removal calls nobody | MATCHES | `..._WHEN_cancel_THEN_another_callback_not_called` | `predictive_cancel` on taken owner returns `Ok(false)` (covered in fallback test) |
| Disabling owner mid-gesture does not break claim | MATCHES | (implied by `isEnabled` checks at selection time) | `disabling_gesture_owner_does_not_interrupt_claim` |
| Mutable `BackCallback.priority` | MATCHES (implemented in this audit) | `var priority` + `GIVEN_..._WHEN_priority_changed...` | `set_priority`/`priority`; `set_priority_changes_winner_selection` |
| Aggregate `isEnabled` + `add/removeEnabledChangedListener` firing only on aggregate flips | MATCHES (implemented in this audit) | `DefaultBackDispatcher.hasEnabledCallback` + ~20 listener tests | `add_enabled_changed_listener`/`remove_enabled_changed_listener`; `enabled_changed_listeners_fire_only_on_aggregate_transitions`; queued `BackCommands` registrations also reconcile |
| Per-callback `isEnabled` listeners | INTENTIONALLY_DIFFERENT | `BackCallback.addEnabledChangedListener` | Enabled changes flow through `set_enabled(id, ...)`; dispatcher-level aggregate listeners cover the platform-sync need. No per-object listener registry. |
| `register`/`unregister` contract (duplicate → throw; unknown → throw; `isRegistered`) | INTENTIONALLY_DIFFERENT | `check(callback !in set)` / `check(callback in set)` | Token IDs: duplicates impossible by construction; `unregister`/`set_enabled` return `bool`. Reentrancy goes through queued `BackCommands`. |
| Progress/cancel/commit with no gesture | INTENTIONALLY_DIFFERENT | no-ops returning `Unit` | `Err(NoGestureInProgress)`. Rationale: explicit wiring-error signal for adapters. |
| `BackEvent.progress` constrained to `0..1` | INTENTIONALLY_DIFFERENT | `require(progress in 0F..1F)` | Stored as given (`progressed` docs "by convention"). Validation left to platforms; no evidence Decompose needs core clamping. |
| `BackHandlerOwner` | INTENTIONALLY_DIFFERENT | `BackHandlerOwner.kt` | No Owner trait (see `DECOMPOSE_READINESS.md`). |
| Nested dispatcher composition (parent/child) | MISSING (deferred, non-blocking) | No upstream parent/child either | No composition abstraction yet. Upstream has none, so this is not a parity gap; Decompose-rs should define propagation before any is introduced. |
| Android `BackHandler(dispatcher)` + `connectOnBackPressedCallback` + lifecycle-scoped variant | SEMANTICALLY_EQUIVALENT within NativeActivity contract | `androidMain/AndroidBackHandler.kt` | `AndroidBackHandler` (API 33 `OnBackInvokedCallback`, API 34+ `OnBackAnimationCallback`, legacy key route) + `AndroidBackBridge` mapping; new aggregate listeners remove the need for manual polling. See `PLATFORM_PARITY.md`. |

## 9. Auxiliary surface

- `utils-internal` (`InternalEssentyApi`, `ExperimentalEssentyApi` opt-in
  markers): NOT_APPLICABLE. Rust uses ordinary semver + docs; experimental
  upstream helpers (`saveable`, `retaining*`) were assessed individually above.
- `state-keeper-benchmarks`: benchmark harness only, no behavior. Rust has
  `docs/BENCHMARK_PLAN.md`; no benchmark evidence is claimed on either side.
- `kotlin-js-store`, `tools`, `docs`, `gradle` infra: build tooling, no
  portable behavior.

## 10. Breaking changes made during this audit (pre-0.1)

1. `InstanceKeeperError::DuplicateKey` added (for `put`).
   Problem: no way to retain a prebuilt instance; `get_or_create` covers only
   factory retention. Why insufficient: component owners (and future Decompose
   contexts) hold already-constructed values. Decompose impact: without `put`,
   owners would wrap factories artificially. Alternatives: reuse `TypeMismatch`
   (wrong meaning), panic (un-Rust-like). Migration: exhaustive `match` on the
   error enum needs a new arm; pre-0.1, documented here.
2. `BackDispatcher` struct grew private fields (`enabled_listeners`,
   `gesture_start_event`, caches). No public layout was promised (opaque
   struct, `Default`/`new` constructors); `Default` still yields
   `has_enabled = false` consistently.
3. `StateKeeper` provider type is now `Option`-returning internally; public
   signatures of existing methods unchanged (`register`/`register_value`
   behave identically). Purely additive surface otherwise.
4. `Subscription::id()` accessor added; `do_on_*` family added. Additive only.

## 11. Genuinely missing (non-blocking, tracked)

- Nested back-dispatcher composition (no upstream equivalent; Decompose-rs
  design task).
- `BackEvent` progress-range clamping (intentional validation difference;
  reversible without breakage if Decompose needs it).
- Executor-neutral async/reactive bridges: implemented as the optional
  `essenty-lifecycle-tokio` crate (scope + repeat in both `Send` and
  thread-local flavors sharing one state machine; streams deferred with
  recorded rationale). No core Tokio dependency; no new executor or reactive
  framework.

## 12. Conclusions

- Core upstream semantic parity: complete (all MATCHES / SEMANTICALLY_EQUIVALENT
  / documented intentional differences; every MATCHES row has a Rust
  regression test).
- Platform integration parity: per `PLATFORM_PARITY.md` Matrix A.
- Auxiliary-module parity: coroutines capability implemented via
  `essenty-lifecycle-tokio` (Kotlin API itself language-specific, not
  transliterated); Reaktive API not ported, cleanup covered by Tokio + RAII;
  serialization helpers intentionally different (serde model).
- Rust-specific extension quality: per `PLATFORM_PARITY.md` Matrix B.
- Runtime verification confidence: per `PLATFORM_PARITY.md` taxonomy.
- Decompose-rs readiness: per `DECOMPOSE_READINESS.md`.
