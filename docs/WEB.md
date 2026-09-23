# Web integration

## Implemented

`BrowserLifecycle::new()` reads the current document visibility and attaches
`visibilitychange`, `pagehide`, and `pageshow` listeners. It updates a Rust
`LifecycleRegistry` and removes all listeners when dropped. A page hidden in
the browser back/forward cache stays restorable; non-persisted page hide
destroys the registry.

```toml
[dependencies]
essenty-web = "0.1"
```

```rust,ignore
use essenty_web::wasm::BrowserLifecycle;

let lifecycle = BrowserLifecycle::new()?;
let _subscription = lifecycle.registry().subscribe(|state| {
    // Rust callback.
});
```

`BrowserLifecycle` is available on `wasm32` only and must run in a browser
window with a document. It does not require consumer JavaScript.

`HistoryBackBridge` is an explicit mapping helper. It does not install a
`popstate` listener or take over browser navigation. `popstate` fires after
the active history entry changes, so a callback cannot cancel that change.
`StorageKey` namescopes keys, but no automatic session or local storage
persistence is installed; applications may continue to use pure Rust
`StateKeeper` with their chosen storage strategy.

## Verified

- Visibility and back mapping unit tests: host-tested.
- Browser listener code: `cargo check --target wasm32-unknown-unknown`.
- Browser runtime tests: not run.

## Planned

- A browser test harness for listener delivery and drop cleanup.
- Optional state persistence with a specified encoding and failure policy.
- An opt-in history strategy if it can preserve predictable navigation.
