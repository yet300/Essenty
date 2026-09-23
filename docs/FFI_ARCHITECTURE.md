# Host and FFI architecture

The Rust `Runtime` owns the root lifecycle, state keeper, instance keeper and back dispatcher. Android, Apple and Web adapters translate native signals into `PlatformEvent` and make one call to `Runtime::dispatch`. Rust components subscribe or register directly with the runtime's core objects during graph construction. The platform holds one opaque runtime handle, not handles for every component.

```text
Activity / scene / browser
    -> thin adapter: native signal -> PlatformEvent
    -> one FFI call: Runtime::dispatch(event)
    -> Rust component graph processes event synchronously
    -> DispatchResult or RuntimeError
    -> thin adapter updates host registration or persistence
```

The current `PlatformEvent` set is limited to lifecycle target state, ordinary back, predictive start/progress/cancel/commit, and save state. Restored state is supplied once during runtime construction as a keyed opaque byte map. No Android `Bundle`, Objective-C object or JavaScript value enters the core. `DispatchResult` returns back handling, current aggregate back availability, or a saved state map. It does not contain UI state or speculative business effects.

## Ownership and lifetime

The runtime handle is owned by one host scope. On Android, a ViewModel or equivalent can retain that single native handle over configuration changes. On a final Activity or navigation scope teardown, dispatch `Lifecycle(Destroyed)` and release the handle; the runtime then ends its InstanceKeeper scope. External Rust `Rc` clones can outlive that scope until their final owner drops them. Process death restoration creates a new runtime from one opaque state payload, with any key-to-bytes encoding defined by the adapter. Android need not know individual internal state keys.

Apple scene/app owners and Web page/application owners follow the same shape. Child lifecycles, if added later, are propagated from the root inside Rust. The platform never sends a lifecycle event independently to each child. Platform adapters must serialize access to a local runtime on its owning thread. Core objects are not universally `Send + Sync`; a future threaded host should add a dedicated wrapper or actor rather than imposing locks on every callback.

## Dispatch rules

One inbound event completes before returning one outbound result. A callback may modify Rust registrations through Rust APIs, but Rust must not synchronously call Kotlin/Swift/JavaScript and then accept a nested call back into the same runtime. Native UI work follows after dispatch returns. Typed errors remain inside Rust until the adapter maps them to stable FFI error codes or messages.

The Rust `PlatformEvent` and `DispatchResult` types are not themselves a stable C ABI. A later binding layer must define versioned wire tags, owned byte buffer transfer and release functions, and handle validity rules. It should marshal one event and one result per call. A fallible dispatch must leave no borrowed pointer crossing the boundary. State save should encode the whole `BTreeMap<String, Vec<u8>>` as one opaque payload at that layer; the current map is a Rust-side representation, not a request for one FFI call per key.

## Predictive back

The core still accepts progress events and delivers them to callbacks. A native adapter should usually ask Rust at gesture start whether it has a semantic back target, animate progress locally, then send cancel or commit. If Rust progress is needed for a particular component, coalesce samples and send only the latest useful value. The adapter must not send every pointer sample by default. If a claimed Rust callback disappears, the core cancels it; the next progress can claim a fallback. The native animator must be prepared to stop or switch its visual target after the next semantic result.

The `can_handle_back` field is an aggregate snapshot after each dispatch. It can drive native callback registration without separate enabled/priority getters. Changes originating entirely in Rust between platform events may require a future batched notification mechanism; that remains an open integration question.

## Future stateful UI layer

A Decompose-like layer is outside this change. When it exists, it should emit whole render snapshots (or coarse changes with explicit versions) after processing input. Bindings should not expose a forest of property getter calls such as `getTitle`, `getLoading`, and `getItems`. This document sets that boundary rule without adding fake UI state to Essenty's current responsibilities.
