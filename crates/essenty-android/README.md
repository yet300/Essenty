# essenty-android

Android adapter for the Essenty Rust runtime: Activity lifecycle, saved state,
and direct platform back dispatch.

## Architecture

`essenty-android` is a Rust-native Android backend built on `android-activity`
with `NativeActivity`, native saved-state bytes, and direct Android platform
APIs.

- **No AndroidX dependency.** The runtime operates directly against Android
  platform APIs and `android-activity`. No custom Gradle library module or
  consumer JNI code is required.
- **NativeActivity lifecycle.** Maps `android-activity` lifecycle events directly
  into `essenty-lifecycle` states (`Created`, `Started`, `Resumed`, `Destroyed`).
- **State preservation.** Persists binary state snapshots into the host's
  `onSaveInstanceState` native saved bytes envelope, surviving Activity
  recreation and process death.
- **Instance retention across configuration changes.** When the Android host
  declares handled `configChanges` in its manifest, the same-thread NativeActivity
  runtime retains objects across configuration events without recreation.
- **Predictive back dispatch.**
  - **API 34+**: Registers `OnBackAnimationCallback` for full predictive gesture
    progress and edge tracking.
  - **API 33**: Registers `OnBackInvokedCallback`.
  - **Pre-33**: Falls back to NativeActivity `KEYCODE_BACK` key events.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
essenty-android = { version = "0.1.0", features = ["native-activity"] }
android-activity = { version = "0.6", features = ["native-activity"] }
```

### Manifest configuration

For instance retention across configuration changes without Activity recreation,
declare handled changes in `AndroidManifest.xml`:

```xml
<activity
    android:name="android.app.NativeActivity"
    android:configChanges="orientation|screenSize|screenLayout|keyboardHidden|uiMode">
```

### Event loop integration

```rust,no_run
use essenty_android::{AndroidBackHandler, NativeActivityLifecycle, NativeActivityState};
use essenty_back_handler::BackDispatcher;
use std::time::Duration;

// In your android_main entry point:
// Initialize lifecycle, state keeper, and back handler attached to AndroidApp.
```

See [the Android documentation](https://github.com/yet300/Essenty/blob/main/docs/ANDROID.md)
for complete details, API level support, and architecture notes.

## License

Apache-2.0. See [LICENSE](https://github.com/yet300/Essenty/blob/main/LICENSE).
