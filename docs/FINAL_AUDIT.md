# Final audit — Essenty-rs pre-Decompose foundation

## Audit identity

```text
Upstream Essenty SHA: c4f1e914185daa21de4867716a102b44b9a945a3 (master, verified 2026-09-24; no advance since task start)
Upstream branch: master (arkivanov/Essenty, read-only reference)
Initial Rust SHA (audit branch base): b3eda9c0f3c5508ca0329996f45ad5abafa883ff (codex/lifecycle-tokio HEAD)
Audit branch: codex/final-essenty-audit
Audit date: 2026-09-24
```

Branch ancestry: the full history is linear —
`main(35e0383)` → `semantic-compatibility-audit` → `rust-native-platform-port` →
`android-predictive-proxy-poc` → `native-android-production` →
`full-essenty-parity-audit` → `lifecycle-tokio(b3eda9c)`.
`codex/lifecycle-tokio` already contained every completed line of work
(zero commits on any other branch outside its ancestry), so the final-audit
branch starts at `b3eda9c` plus the fixes listed below. No merges,
cherry-picks, or abandoned AndroidX branches were needed.

## Core parity (vs upstream source + tests, not README)

| Module | Verdict | Rationale |
|---|---|---|
| Lifecycle | PASS | State model, transition ordering, forward/reverse callbacks, replay, subscribe/unsubscribe incl. during dispatch, destroy walk, reentrancy, all `do_on_*` match; strict drivers, `AlreadyDestroyed`, token identity are documented intentional differences (`SEMANTIC_COMPATIBILITY.md`). |
| Lifecycle Tokio | PASS | `LifecycleScope` spawn/spawn_local/try_new + `repeat_on_lifecycle(_local)` match upstream `lifecycle-coroutines` capability (cancel/restart/subscription cleanup, `Send` vs `!Send`); Reaktive cleanup covered by cancellation + RAII; `Flow.withLifecycle` deferred as documented convenience adapter. |
| StateKeeper | PASS | Registration, optional providers, duplicates, unregister, `is_registered`, consume-once, save ordering (`BTreeMap`), typed errors, serde codec freedom, malformed restoration all match or are documented intentional differences. |
| InstanceKeeper | PASS | Lookup, identity, type mismatch, put/remove/duplicate, get/create, destruction, exact-once, `Rc` lifetimes match; `destroy_all` clear-vs-retain, `Drop`-vs-`onDestroy`, `String` keys are documented intentional differences. |
| BackHandler | PASS (post-fix) | Registration, enabled + listeners, priority + updates, ordering, ordinary + predictive (start/progress/cancel/invoke), gesture ownership, mid-gesture removal, fallback replay, position/edge, reentrancy all match; `NoGestureInProgress` errors and unvalidated progress range are documented intentional differences. Fixed: `unregister` return value when cancellation self-removes (regression test added). |

Detailed matrices: `docs/SEMANTIC_COMPATIBILITY.md`, `docs/UPSTREAM_PARITY_AUDIT.md`.

## Async classification (final)

```text
Kotlin CoroutineScope API → language-specific; lifecycle-aware async capability → SEMANTICALLY_EQUIVALENT via essenty-lifecycle-tokio.
Reaktive Observable/Disposable API → Kotlin ecosystem-specific; lifecycle-bound cancellation → covered by Tokio integration + Rust RAII.
Flow.withLifecycle → convenience adapter deferred; underlying repeat capability implemented (lib.rs documents the decision).
```

## Platform parity

| Platform | Core parity | Native integration parity | Rust extensions | Verification level |
|---|---|---|---|---|
| Android | YES all four | PARTIAL (Lifecycle YES, State YES, Back YES on 34+/33/native-key, Instance semantic-only for declared config) | `native_config` diagnostics, `AndroidBackStrategy` | EMULATOR API 36.1 (lifecycle, 50 config events, process-death restore, back cancel/invoke/teardown); API 33/34-phone/pre-33 compile-only, documented |
| iOS/tvOS | YES | YES event-for-event (+extra launch observer, sync-vs-async init, RAII teardown — all documented) | shared `AppleLifecycle` helper | COMPILE + host mapping tests; simulator/device UNVERIFIED, documented |
| watchOS/macOS | YES core (upstream core-only) | NOT_APPLICABLE (no upstream integration) | WatchKit/AppKit observers | macOS HOST (synthetic delivery); watchOS COMPILE |
| Web | YES core (upstream core-only) | NOT_APPLICABLE | visibility/history/storage + wasm bindings | UNIT (mapping) + wasm COMPILE; browser UNVERIFIED |
| Linux x64 | YES (core-only upstream) | trivially YES | — | UNIT |
| JVM | NOT_APPLICABLE (language target) | — | — | — |
| Windows / musl+ARM64 / visionOS / Catalyst | extension only | — | correctly Matrix B, never claimed as parity | UNIT (x64) else COMPILE |

Upstream parity matrix vs Rust extension matrix stay separate: `docs/PLATFORM_PARITY.md`
(Matrix A vs B), `docs/TARGETS.md` (21-triple compile matrix).

## Fixes applied during this final audit

1. `BackDispatcher::unregister` returned `false` for an existing gesture owner
   whose `Cancelled` callback self-removed via `BackCommands`
   (`crates/essenty-back-handler/src/dispatcher.rs`). Fixed by checking
   existence first; regression test
   `unregister_gesture_owner_that_self_removes_on_cancel_returns_true` added.
2. `essenty-web` listed `wasm-bindgen`/`js-sys`/`web-sys` as unconditional
   dependencies, pulling the wasm graph into host builds despite correctly
   gated code. Moved under `[target.'cfg(target_arch = "wasm32")'.dependencies]`;
   verified host `cargo tree` is clean and wasm target still resolves.
3. `cargo doc` warning (redundant explicit link in `essenty-lifecycle-tokio`
   docs). Fixed.
4. Docs: recorded progress-range tolerance and Apple RAII-teardown as
   intentional differences in `docs/SEMANTIC_COMPATIBILITY.md`; noted wasm
   target-gating in `essenty-web` docs.

No other definite bugs found. Strict-vs-idempotent drivers, `Drop`-vs-`onDestroy`,
`bool`-vs-throw returns, and error-vs-noop back stray events remain intentional,
documented, and unchanged per the no-redesign principle.

## Quality

```text
Tests: PASS (workspace + --all-features; 25 lifecycle, 17 back-handler, 13 instance-keeper, 14 state-keeper, 19 android, 3 apple, 22 tokio integration, facade/runtime, doctests)
Clippy: PASS (--workspace --all-targets --all-features -D warnings)
Docs: PASS (cargo doc --workspace --no-deps, zero warnings)
Fmt: PASS (cargo fmt --all -- --check)
Unsafe audit: PASS (zero unsafe in core, workspace deny; adapter unsafe confined with SAFETY + cleanup paths)
Dependency audit: PASS (tokio only via essenty-lifecycle-tokio; JNI/objc2/wasm gated; host web graph fixed)
Secrets scan: PASS (zero hits; .gitignore covers target/.DS_Store/.env)
Target matrix: PASS (21/21 cargo check; native-activity on both Android ABIs; tokio on linux/darwin/windows/wasm/android samples)
No AndroidX production dependency: PASS (only historical/comparison mentions)
No platform types in core: PASS | No mandatory Send+Sync: PASS
Working tree: CLEAN at merge
```

## Known non-blocking limitations (documented, not hidden)

- API 33 / API 34 phone / pre-33 Android runtime unverified (no runnable images); sound implementation + compile checks; see `docs/ANDROID.md`.
- iOS/tvOS/watchOS simulator/device delivery unverified; compile + host mapping tests; see `docs/APPLE.md`.
- Browser delivery unverified; mapping unit tests + wasm compile; see `docs/WEB.md`.
- `Flow.withLifecycle`-style stream adapter deferred (repeat capability present).
- No nested parent/child back-dispatcher composition yet (component runtime to define it).

## Decompose readiness

```text
READY
```

`Lifecycle`, `StateKeeper`, `InstanceKeeper`, `BackHandler` (+ optional
`LifecycleScope`/`repeat_on_lifecycle(_local)`) compose without JNI/ObjC/wasm
types, singletons, mandatory Tokio, mandatory `Send+Sync`, or Android host
types (verified by grep + `cargo tree`; see `docs/DECOMPOSE_READINESS.md`).
Decompose-rs itself is out of scope for this task.

## Git

Recorded at push time (see final report message for SHAs and verification).
