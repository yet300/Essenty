# Platform parity

Two matrices, kept separate on purpose. Matrix A covers only
capabilities/platforms present upstream at
`c4f1e914185daa21de4867716a102b44b9a945a3`. Matrix B covers Rust extensions
that must not inflate parity claims. Core-vs-native-integration is
distinguished in every row: compiling the core on a platform is not the same
as integrating with that platform's OS lifecycle/state/back APIs.

Runtime taxonomy used below: `UNIT_TESTED`, `HOST_RUNTIME_TESTED`,
`SIMULATOR_TESTED`, `EMULATOR_TESTED`, `DEVICE_TESTED`, `COMPILE_TESTED`,
`UNVERIFIED`. `cargo check` never counts as runtime support.

Upstream target families (from `build.gradle.kts` `setupDefaults`):
Android, JVM, JS (browser+nodejs), wasmJs (browser), iOS, watchOS, tvOS, macOS,
Linux x64; `minSdkVersion = 15`.

Rust target matrix (21 triples, re-verified 2026-09-24 in this audit, all
`cargo check --workspace --target` PASS): Android
`aarch64-linux-android`/`x86_64-linux-android` (+ `native-activity` feature
check on both); iOS `aarch64-apple-ios`/`aarch64-apple-ios-sim`; macOS
`aarch64-apple-darwin`/`x86_64-apple-darwin`; Windows
`x86_64-pc-windows-msvc`/`aarch64-pc-windows-msvc`; Linux GNU
`x86_64-unknown-linux-gnu`/`aarch64-unknown-linux-gnu`; Linux musl
`x86_64-unknown-linux-musl`/`aarch64-unknown-linux-musl`; Web
`wasm32-unknown-unknown`; watchOS `aarch64-apple-watchos`/`-sim`; tvOS
`aarch64-apple-tvos`/`-sim`; visionOS `aarch64-apple-visionos`/`-sim`;
Catalyst `aarch64-apple-ios-macabi`/`x86_64-apple-ios-macabi`.

## Matrix A — upstream parity

| Platform | Upstream core parity | Upstream native integration parity | Rust extensions on this platform | Runtime verification |
|---|---|---|---|---|
| Android | YES — all four primitives | PARTIAL — Lifecycle: YES (`NativeActivityLifecycle`, idempotent host driving); State: YES (versioned/checksummed envelope, restore/save); Instance: SEMANTICALLY_EQUIVALENT only for declared `configChanges` (arbitrary recreation UNSUPPORTED for non-`Send`); Back: YES on API 34+ paths, UNVERIFIED on API 33/pre-33 images | `native_config` diagnostics; `AndroidBackStrategy` selector; aggregate-listener sync | EMULATOR_TESTED API 36.1 (lifecycle, config retention incl. 50-event run, process-death restore, back attach/cancel/invoke/teardown); API 33/pre-33 COMPILE_TESTED only |
| iOS | YES — all four primitives | YES — `ApplicationLifecycle` event-for-event: finish-launch→Created, foreground→Started, active→Resumed, resign→Started, background→Created, terminate→destroy; initial state mapped on main queue; observer removed on drop; explicit `destroy()` walks Created first | — (shared `AppleLifecycle` mapping helper) | COMPILE_TESTED (`aarch64-apple-ios`, `-sim`); HOST_RUNTIME_TESTED for shared mapping only; device/simulator delivery UNVERIFIED |
| tvOS | YES — all four primitives | YES — shares the UIKit implementation with iOS (same notifications, same mapping; upstream `itvosMain` is likewise shared) | — | COMPILE_TESTED (`aarch64-apple-tvos`, `-sim`); runtime UNVERIFIED (not inferred from iOS) |
| watchOS | YES — core only (upstream has NO watchOS `ApplicationLifecycle`; only common core compiles there) | NOT_APPLICABLE upstream — nothing to match | Rust WatchKit observer → Matrix B | COMPILE_TESTED (`aarch64-apple-watchos`, `-sim`); runtime UNVERIFIED |
| macOS | YES — core only (upstream has NO macOS lifecycle integration) | NOT_APPLICABLE upstream — nothing to match | Rust AppKit observer → Matrix B (HOST_RUNTIME_TESTED: synthetic delivery + removal) | macOS observer HOST_RUNTIME_TESTED; other macOS rows COMPILE_TESTED |
| JS + wasmJs | YES — core only (upstream `js`/`wasmJs` targets compile common code; pinned source contains NO browser lifecycle/storage/history integration) | NOT_APPLICABLE upstream — nothing to match | Rust `VisibilityLifecycle`, `HistoryBackBridge`, `StorageKey`, wasm bindings → Matrix B | `essenty-web` host tests UNIT_TESTED; `wasm32-unknown-unknown` COMPILE_TESTED; browser delivery UNVERIFIED |
| JVM | Core compiles upstream; NO Rust analogue (language/runtime target, not a platform gap) | NOT_APPLICABLE | Rust desktop/native targets (Linux/Windows/macOS) provide the same core capabilities for the Rust ecosystem | UNIT_TESTED on Linux hosts |
| Linux x64 | YES — all four primitives (core-only upstream target) | YES — trivially (upstream has no Linux-native adapters; none required) | ARM64 GNU/musl → Matrix B | UNIT_TESTED (`x86_64-unknown-linux-gnu` runs full suite) |

Explicit answers:

- Does every upstream-supported platform have at least the same core Essenty
  functionality in Rust? **YES** (Android, JVM-excepted-as-language-target,
  JS/wasmJs-core, iOS, watchOS-core, tvOS, macOS-core, Linux x64; JVM core is
  available to Rust developers via Linux/Windows/macOS targets).
- Does every upstream platform-specific integration have a semantic Rust
  equivalent? **PARTIAL**: Android YES within the documented NativeActivity
  contract (with the config-change scoping limit); iOS/tvOS YES by
  event-mapping (runtime delivery unverified on simulator/device); watchOS and
  macOS have no upstream integration to match (Rust ships more).

## Matrix B — Rust extensions (not parity)

| Extension | Targets / area | Verification |
|---|---|---|
| Windows core support (upstream has no Windows target) | `x86_64-pc-windows-msvc` (suite runs), `aarch64-pc-windows-msvc` (check) | UNIT_TESTED (x64), COMPILE_TESTED (ARM64) |
| Linux ARM64 GNU + musl; musl x64 | `aarch64-unknown-linux-gnu`, `x86_64/aarch64-unknown-linux-musl` | COMPILE_TESTED |
| visionOS lifecycle observer | `aarch64-apple-visionos`, `-sim` | COMPILE_TESTED |
| Catalyst lifecycle observer | `aarch64/x86_64-apple-ios-macabi` | COMPILE_TESTED |
| watchOS WatchKit observer (upstream: core only) | `aarch64-apple-watchos`, `-sim` | COMPILE_TESTED; correctness reviewed, delivery UNVERIFIED |
| macOS AppKit observer (upstream: core only) | `aarch64/x86_64-apple-darwin` | HOST_RUNTIME_TESTED (synthetic notification delivery + observer removal) |
| `BrowserLifecycle` visibility/pagehide mapping | `essenty-web` host + wasm | UNIT_TESTED (mapping), COMPILE_TESTED (wasm), browser delivery UNVERIFIED |
| `BrowserStorage` session/local hex persistence | `essenty-web` host + wasm | UNIT_TESTED (mapping), browser delivery UNVERIFIED |
| `BrowserHistoryBack` popstate bridge (cannot cancel navigation — documented, not Android semantics) | `essenty-web` host + wasm | UNIT_TESTED (mapping), browser delivery UNVERIFIED |
| Optional `Runtime`/`PlatformEvent`/`DispatchResult` host entry point | `essenty` crate, `runtime` feature | UNIT_TESTED |
| `native_config` manifest diagnostics, `AndroidBackStrategy` | `essenty-android` | UNIT_TESTED (parsing), COMPILE_TESTED (device readout) |

## Min-SDK / platform floors

- Upstream: `minSdkVersion = 15` (`build.gradle.kts`).
- Rust: `min_sdk_version = 23` (tested floor of the runtime harness,
  `docs/ANDROID.md`). Classified as a **compatibility/platform-support
  difference**, not a semantic gap: the Rust backend uses `android-activity`
  NativeActivity APIs and `jni-min-helper` proxy support whose floors sit at
  23. The min SDK was NOT lowered for this audit — matching the number without
  dependency and implementation support would be dishonest.

## Platform guides

`docs/ANDROID.md`, `docs/APPLE.md`, `docs/WEB.md` hold the per-platform host
contracts and verification boundaries; `docs/TARGETS.md` holds the triple
list. This file is the parity verdict; those files are the operating manuals.
