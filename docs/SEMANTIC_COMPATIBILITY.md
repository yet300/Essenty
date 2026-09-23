# Semantic compatibility with Essenty

Reference: upstream Essenty commit [`c4f1e914185daa21de4867716a102b44b9a945a3`](https://github.com/arkivanov/Essenty/tree/c4f1e914185daa21de4867716a102b44b9a945a3), inspected 2026-09-23. This is a behavioral comparison, not a Kotlin API port. A `MATCHES` label means a Rust regression test covers the stated behavior; it does not claim general equivalence.

## Lifecycle

Upstream references: [registry implementation](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/lifecycle/src/commonMain/kotlin/com/arkivanov/essenty/lifecycle/LifecycleRegistryImpl.kt), [registry extensions](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/lifecycle/src/commonMain/kotlin/com/arkivanov/essenty/lifecycle/LifecycleRegistryExt.kt), [tests](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/lifecycle/src/commonTest/kotlin/com/arkivanov/essenty/lifecycle/LifecycleRegistryTest.kt).

| Behavior | Status | Rust evidence / decision |
|---|---|---|
| Initialized, Created, Started, Resumed, Destroyed; adjacent forward and reverse events | MATCHES | `full_forward_and_backward_cycle`, `move_to_walks_intermediate_states_in_order` |
| New observers catch up with Created/Started/Resumed callbacks for the current state | MATCHES | `reentrant_subscribe_from_callback_is_safe`, `destroy_notifies_observers_once` exercise replay; Rust reports state values instead of callback method names |
| Forward callbacks in registration order; pause, stop and destroy in reverse order | MATCHES | `reverse_events_notify_in_reverse_subscription_order` |
| Unsubscribe from callback; callback list snapshot for current event | MATCHES | `reentrant_subscribe_from_callback_is_safe`, `dropping_subscription_unsubscribes`; additions take effect on later events |
| Destroy from Resumed emits pause, stop, destroy; terminal registry clears observers | MATCHES | `destroy_notifies_observers_once`, `reverse_events_notify_in_reverse_subscription_order` |
| Callback can destroy registry during another callback | MATCHES | `callback_can_destroy_during_create` |
| Strict event methods reject wrong predecessor | MATCHES | `invalid_strict_steps_are_rejected` |
| Higher-level drive methods tolerate repeated events | INTENTIONALLY DIFFERENT | Rust uses `move_to` for idempotent host driving while `create/start/resume/pause/stop` are explicitly strict. This keeps invalid manual transitions observable as typed errors. Adapter tests cover duplicate host callbacks. |
| Repeated destroy | INTENTIONALLY DIFFERENT | Rust returns `AlreadyDestroyed`; upstream convenience `destroy()` is a no-op. A typed terminal error exposes accidental double teardown to direct callers. |
| Duplicate observer identity | INTENTIONALLY DIFFERENT | Rust registrations have unique tokens and may use the same closure value repeatedly. Kotlin rejects subscribing the same callback object twice. Tokens make removal unambiguous. |
| Reentrancy where a callback drives a different transition mid-dispatch | TODO | `move_to` re-reads state after every callback event, but the exact nested notification order across arbitrary reentrant transitions needs a dedicated compatibility matrix. |
| Return to Initialized after creation | INTENTIONALLY DIFFERENT | Rust rejects it; upstream transition graph has no reverse edge to Initialized. |

## StateKeeper

Upstream references: [dispatcher](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/state-keeper/src/commonMain/kotlin/com/arkivanov/essenty/statekeeper/DefaultStateKeeperDispatcher.kt), [tests](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/state-keeper/src/commonTest/kotlin/com/arkivanov/essenty/statekeeper/DefaultStateKeeperDispatcherTest.kt).

| Behavior | Status | Rust evidence / decision |
|---|---|---|
| Unique provider keys; duplicate registration rejected | MATCHES | `duplicate_registration_is_rejected` |
| Restored value can be consumed once | MATCHES | `consume_is_single_shot` |
| Providers evaluated at save time | MATCHES | `bytes_round_trip`, `encode_failure_surfaces_at_save` |
| Unconsumed restored entries survive another save; live providers overwrite same keys | MATCHES | `unconsumed_restored_values_survive_another_save` |
| Unregister leaves restored entry available | MATCHES | `unregister_removes_provider_but_keeps_restored` |
| Unregister of absent key | INTENTIONALLY DIFFERENT | Rust returns `false`; Kotlin throws. Rust's explicit result makes idempotent cleanup practical. |
| Provider may return no state | TODO | Kotlin skips a supplier returning null, preserving an older unconsumed restored value. Current Rust byte provider must return bytes; no skip variant exists. |
| Provider mutates registration during save | NOT APPLICABLE | Rust `save(&self)` and `register(&mut self)` exclude this through the public API; a provider cannot borrow the same keeper mutably safely during save. |
| Codec ownership and failure | INTENTIONALLY DIFFERENT | Rust stores opaque bytes and lets callers choose codecs. Errors are typed; Kotlin stores serializable containers and serializer strategies. `serde_value_round_trip_with_pluggable_codec` and failure tests cover Rust behavior. |
| Save order | INTENTIONALLY DIFFERENT | Rust `BTreeMap` evaluates providers by key, yielding reproducible snapshots. Kotlin `HashMap` does not promise a stable iteration order. `multiple_keys_save_in_order` covers Rust ordering. |

## InstanceKeeper

Upstream references: [contract](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/instance-keeper/src/commonMain/kotlin/com/arkivanov/essenty/instancekeeper/InstanceKeeper.kt), [dispatcher](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/instance-keeper/src/commonMain/kotlin/com/arkivanov/essenty/instancekeeper/DefaultInstanceKeeperDispatcher.kt), [tests](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/instance-keeper/src/commonTest/kotlin/com/arkivanov/essenty/instancekeeper/DefaultInstanceKeeperDispatcherTest.kt).

| Behavior | Status | Rust evidence / decision |
|---|---|---|
| Same key yields same retained instance; factory runs once | MATCHES | `get_or_create_returns_same_instance` |
| Missing key has no value; multiple keys independent | MATCHES | `get_returns_none_until_created`, `multiple_keys_are_independent` |
| Terminal scope destruction is idempotent and rejects new retention | INTENTIONALLY DIFFERENT | `destroy_all` ends Rust scope; unlike upstream post-destroy `put`, Rust refuses to retain an immediately doomed value. `terminal_destroy_is_idempotent_and_rejects_new_instances` |
| Releasing a key permits a new value | MATCHES | `destroy_releases_and_allows_recreation` (Rust key removal is named `destroy`) |
| Object cleanup at end of scope | INTENTIONALLY DIFFERENT | Rust `Drop` runs only after the last external `Rc` clone is released. Kotlin calls `onDestroy` at dispatcher scope end even when callers still reference the object. `value_drops_when_keeper_and_clones_are_gone` covers this ownership rule. |
| Keys and type mismatch | INTENTIONALLY DIFFERENT | Rust uses string keys and returns `TypeMismatch`; Kotlin accepts `Any` keys and casts in typed helpers. `type_mismatch_returns_error_and_keeps_value` |
| Explicit put/remove of prebuilt instance | TODO | Current Rust API is get/create oriented. Add only if a concrete component owner needs it. |

## BackHandler

Upstream references: [dispatcher](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/back-handler/src/commonMain/kotlin/com/arkivanov/essenty/backhandler/DefaultBackDispatcher.kt), [callback](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/back-handler/src/commonMain/kotlin/com/arkivanov/essenty/backhandler/BackCallback.kt), [tests](https://github.com/arkivanov/Essenty/blob/c4f1e914185daa21de4867716a102b44b9a945a3/back-handler/src/commonTest/kotlin/com/arkivanov/essenty/backhandler/DefaultBackDispatcherTest.kt).

| Behavior | Status | Rust evidence / decision |
|---|---|---|
| Highest enabled priority, then latest registration, handles back | MATCHES | `highest_priority_wins`, `later_registration_breaks_priority_ties`, `disabled_handler_is_skipped` |
| Gesture claims selected handler; later registration does not steal it | MATCHES | `gesture_sticks_to_claimed_handler` |
| Removing selected handler cancels it; later progress selects fallback | MATCHES | `removed_gesture_owner_is_cancelled_and_fallback_starts_on_progress` |
| Disabling selected handler does not break in-flight claim | MATCHES | `disabling_gesture_owner_does_not_interrupt_claim` |
| Handler changes registration during callback | INTENTIONALLY DIFFERENT | Rust queues changes through `BackCommands` and applies them immediately after callback return, avoiding mutable aliasing. `callback_can_unregister_itself_and_register_successor` covers self-removal and registration. |
| Progress carries gesture position and swipe edge | INTENTIONALLY DIFFERENT | Core currently carries progress only. Native visual animation can retain edge/coordinates; a future semantic consumer can justify adding them. |
| Progress/cancel/commit without start | INTENTIONALLY DIFFERENT | Rust reports `NoGestureInProgress`; upstream ignores stray progress/cancel. An explicit error helps adapters detect wiring mistakes. |
| Dynamic priority changes | TODO | Rust priority is fixed at registration; upstream callback priority is mutable. |
| Aggregate enabled-change listeners | TODO | Rust exposes `can_handle` in each runtime result. A push listener may be needed for native back registration when Rust callbacks change enabled state between platform events. |
| Nested back dispatch and callback replacement during cancellation | TODO | The command queue prevents aliasing, but deeply nested behavior has not been exhaustively compared to upstream. |
| Nested dispatcher composition | TODO | No parent/child back dispatcher abstraction exists yet. A component runtime should define propagation and ownership before introducing one. |

## Threading, allocation and portability

| Type | Thread property | Main allocations | Portability |
|---|---|---|---|
| `LifecycleRegistry`, `Subscription` | neither `Send` nor `Sync` (`Rc<RefCell<_>>`) | observer `Rc`, snapshot `Vec` per event | currently `std`; uses mostly `core`/`alloc` APIs |
| `StateKeeper` | no universal `Send`/`Sync` guarantee (provider trait objects) | keyed `BTreeMap`, provider boxes, saved byte vectors | currently `std`; serde is `alloc` capable, error derive/config needs review |
| `InstanceKeeper` | neither `Send` nor `Sync` (`Rc`) | retained `Rc`, `BTreeMap` keys | `Any` downcasting and current imports use `std` |
| `BackDispatcher` | no universal `Send`/`Sync` guarantee (callback trait objects) | callback boxes, keyed entries, command `Vec` | currently `std`; core/alloc conversion looks feasible |
| `Runtime` | inherits local thread confinement | owns four core values | host must dispatch on its owning thread |

`Rc`, `Box<dyn Fn...>`, and keyed maps are appropriate for independently owned callbacks and heterogeneously typed retained instances. There is no benchmark evidence to justify a container rewrite. See `BENCHMARK_PLAN.md` for baseline scenarios. No crate is claimed `no_std` compatible yet; converting all error handling, imports, and platform adapter assumptions together is safer than a partial claim.
