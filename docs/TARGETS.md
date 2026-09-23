# Target matrix

The requested canonical matrix contains **21** triples: ten two-triple
families plus one Web triple. Counting only the ten paired families gives
**20**; adding `wasm32-unknown-unknown` gives **21**. This explains the
arithmetic discrepancy. The prior audit did not record which row its 20-count
omitted, so the historical source of that count cannot be established more
precisely.

| Family | Target triples |
|---|---|
| Android | `aarch64-linux-android`, `x86_64-linux-android` |
| iOS | `aarch64-apple-ios`, `aarch64-apple-ios-sim` |
| macOS | `aarch64-apple-darwin`, `x86_64-apple-darwin` |
| Windows | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` |
| Linux GNU | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Linux musl | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` |
| Web | `wasm32-unknown-unknown` |
| watchOS | `aarch64-apple-watchos`, `aarch64-apple-watchos-sim` |
| tvOS | `aarch64-apple-tvos`, `aarch64-apple-tvos-sim` |
| visionOS | `aarch64-apple-visionos`, `aarch64-apple-visionos-sim` |
| Mac Catalyst | `aarch64-apple-ios-macabi`, `x86_64-apple-ios-macabi` |

On 2026-09-23, `cargo check --workspace --target <triple>` passed locally for
all 21 triples. The `essenty-android/native-activity` feature also passed for
both listed Android triples.

`cargo check` is a compile check only. Host tests exercise the four core
primitives and mapping helpers. Neither cross-compilation nor a simulator SDK
proves that a notification, Activity callback, or browser event was delivered
at runtime. See the platform guides for those separate statuses.
