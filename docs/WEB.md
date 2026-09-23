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

`BrowserHistoryBack::new()` installs an opt-in `popstate` listener and removes
it on drop. Register callbacks through `browser_history.bridge()`. `popstate`
fires after the active history entry changes, so a callback cannot cancel that
change. The adapter does not add synthetic history entries.

`BrowserStorage` provides explicit session or local storage persistence for
opaque `StateKeeper` bytes. `load_keeper()` restores a keeper, and
`save_keeper(&keeper)` captures its providers. Each namespace uses lowercase
hex values; malformed data and storage access failures return typed errors.
Web Storage has no transactions, so a failed save can leave a partial snapshot.
Applications choose when to save and which storage area to use.

```rust,ignore
use essenty_web::{StorageArea, StorageKey};
use essenty_web::wasm::{BrowserHistoryBack, BrowserStorage};

let history = BrowserHistoryBack::new()?;
history.bridge().borrow_mut().dispatcher_mut().register(0, true, |_| {});

let storage = BrowserStorage::new(StorageKey::new(StorageArea::Session, "main"))?;
let mut keeper = storage.load_keeper()?;
keeper.register("counter", || 42_u32.to_le_bytes().to_vec())?;
storage.save_keeper(&keeper)?;
```

## Verified

- Visibility and back mapping unit tests: host-tested.
- Browser lifecycle, history, and storage bindings: `cargo check --target wasm32-unknown-unknown`.
- Hex encoding and corruption rejection: host-tested.
- Browser runtime tests: not run.

## Planned

- A browser test harness for listener delivery and drop cleanup.
- A browser test harness for storage access and `popstate` delivery.
