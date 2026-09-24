//! Direct `android.window` callback integration for `NativeActivity` hosts.

#![allow(unsafe_code)]

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use android_activity::AndroidApp;
use essenty_back_handler::{BackDispatcher, GesturePosition, SwipeEdge};
use jni::{
    Env, JavaVM, jni_sig, jni_str,
    objects::{JObject, JValue},
    refs::{Global, LoaderContext},
    signature::RuntimeMethodSignature,
};
use jni_min_helper::DynamicProxy;

use crate::AndroidBackBridge;

/// Runtime strategy selected for this Android device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidBackStrategy {
    /// `KEYCODE_BACK` delivered by the `NativeActivity` input queue.
    NativeKey,
    /// `OnBackInvokedCallback`, introduced in API 33.
    InvokedCallback,
    /// `OnBackAnimationCallback`, introduced in API 34.
    PredictiveCallback,
}

impl AndroidBackStrategy {
    /// Selects the only active back mechanism for the given API level.
    #[must_use]
    pub const fn for_api_level(api: i32) -> Self {
        if api >= 34 {
            Self::PredictiveCallback
        } else if api >= 33 {
            Self::InvokedCallback
        } else {
            Self::NativeKey
        }
    }
}

/// Errors reported by asynchronous Android callback setup or teardown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidBackError {
    /// Operation that failed.
    pub operation: &'static str,
    /// JNI or platform error text.
    pub detail: String,
}

impl std::fmt::Display for AndroidBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Android back {} failed: {}", self.operation, self.detail)
    }
}

impl std::error::Error for AndroidBackError {}

#[derive(Debug)]
enum PlatformEvent {
    Started(GesturePosition, f32),
    Progressed(GesturePosition, f32),
    Cancelled,
    Invoked,
}

struct Registration {
    dispatcher: Global<JObject<'static>>,
    proxy: DynamicProxy,
}

type RegistrationRef = Arc<Mutex<Option<Registration>>>;
type RegistrationRetries = Mutex<Vec<RegistrationRef>>;

static UNREGISTER_RETRY: OnceLock<RegistrationRetries> = OnceLock::new();

impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration").finish_non_exhaustive()
    }
}

impl Registration {
    fn unregister(&self, env: &mut Env<'_>) -> Result<(), jni::errors::Error> {
        let signature =
            RuntimeMethodSignature::from_str("(Landroid/window/OnBackInvokedCallback;)V")?;
        env.call_method(
            &self.dispatcher,
            jni_str!("unregisterOnBackInvokedCallback"),
            signature.method_signature(),
            &[JValue::Object(&self.proxy)],
        )?;
        Ok(())
    }
}

/// `NativeActivity` back adapter. Platform callbacks only enqueue events; the
/// caller drains them on its Rust event-loop thread into the owned dispatcher.
///
/// Registration/unregistration are always posted to Android's Java main
/// thread. The adapter owns the proxy and dispatcher global reference until
/// unregister succeeds. Drop posts teardown and closes the Rust event receiver,
/// so delayed callbacks cannot access the destroyed dispatcher.
#[derive(Debug)]
pub struct AndroidBackHandler {
    app: AndroidApp,
    bridge: AndroidBackBridge,
    sender: mpsc::Sender<PlatformEvent>,
    receiver: mpsc::Receiver<PlatformEvent>,
    registration: Arc<Mutex<Option<Registration>>>,
    error: Arc<Mutex<Option<AndroidBackError>>>,
    registered: Arc<AtomicBool>,
    strategy: AndroidBackStrategy,
    callback_count: u64,
    closed: bool,
}

impl AndroidBackHandler {
    /// Creates a handler and schedules platform registration on the Java main
    /// thread. Call [`Self::take_error`] to inspect asynchronous setup failures.
    #[must_use]
    pub fn attach(app: &AndroidApp, dispatcher: BackDispatcher) -> Self {
        let (sender, receiver) = mpsc::channel();
        let registration = Arc::new(Mutex::new(None));
        let error = Arc::new(Mutex::new(None));
        let registered = Arc::new(AtomicBool::new(false));
        let strategy = AndroidBackStrategy::for_api_level(jni_min_helper::android_api_level());
        if strategy != AndroidBackStrategy::NativeKey && dispatcher.can_handle() {
            registered.store(true, Ordering::Release);
            install(
                app,
                sender.clone(),
                Arc::clone(&registration),
                Arc::clone(&error),
                Arc::clone(&registered),
                strategy,
            );
        }
        Self {
            app: app.clone(),
            bridge: AndroidBackBridge::with_dispatcher(dispatcher),
            sender,
            receiver,
            registration,
            error,
            registered,
            strategy,
            callback_count: 0,
            closed: false,
        }
    }

    /// Selected platform back strategy.
    #[must_use]
    pub const fn strategy(&self) -> AndroidBackStrategy {
        self.strategy
    }

    /// Mutable core dispatcher for Rust handler registration.
    #[must_use]
    pub fn dispatcher_mut(&mut self) -> &mut BackDispatcher {
        self.bridge.dispatcher_mut()
    }

    /// Shared core dispatcher.
    #[must_use]
    pub fn dispatcher(&self) -> &BackDispatcher {
        self.bridge.dispatcher()
    }

    /// Number of platform callbacks delivered to this Rust event loop.
    #[must_use]
    pub const fn callback_count(&self) -> u64 {
        self.callback_count
    }

    /// Takes one asynchronous JNI setup or unregister failure, if present.
    #[must_use]
    pub fn take_error(&self) -> Option<AndroidBackError> {
        self.error.lock().ok()?.take()
    }

    /// Unregisters on the Java main thread and waits for completion. Call this
    /// from the `NativeActivity` Rust event-loop thread, never from a Java
    /// callback. On failure, the proxy is quarantined for a later cleanup
    /// attempt so Java cannot call freed Rust handler state.
    ///
    /// # Errors
    /// Returns an error if a gesture is still active or Java fails to unregister.
    pub fn close(&mut self) -> Result<(), AndroidBackError> {
        if self.bridge.dispatcher().has_active_gesture() {
            return Err(AndroidBackError {
                operation: "unregister",
                detail: "a predictive gesture is still owned by the Rust dispatcher".to_owned(),
            });
        }
        if self.strategy == AndroidBackStrategy::NativeKey {
            self.closed = true;
            return Ok(());
        }
        let registration = Arc::clone(&self.registration);
        let registered = Arc::clone(&self.registered);
        let error = Arc::clone(&self.error);
        let (complete, result) = mpsc::sync_channel(1);
        self.app.run_on_java_main_thread(Box::new(move || {
            let outcome = jni_min_helper::jni_with_env(|env| {
                let mut slot = registration
                    .lock()
                    .map_err(|_| jni::errors::Error::NullPtr("back registration mutex poisoned"))?;
                if let Some(value) = slot.as_ref() {
                    value.unregister(env)?;
                    slot.take();
                }
                Ok::<_, jni::errors::Error>(())
            });
            let outcome = match outcome {
                Ok(()) => {
                    registered.store(false, Ordering::Release);
                    remove_quarantined(&registration);
                    Ok(())
                }
                Err(cause) => {
                    clear_pending_java_exception();
                    registered.store(true, Ordering::Release);
                    if let Ok(mut retries) =
                        UNREGISTER_RETRY.get_or_init(|| Mutex::new(Vec::new())).lock()
                    {
                        retries.push(Arc::clone(&registration));
                    }
                    let failure =
                        AndroidBackError { operation: "unregister", detail: cause.to_string() };
                    if let Ok(mut slot) = error.lock() {
                        *slot = Some(failure.clone());
                    }
                    Err(failure)
                }
            };
            let _ = complete.send(outcome);
        }));
        let completed = result.recv().unwrap_or_else(|_| {
            Err(AndroidBackError {
                operation: "unregister",
                detail: "Java main-thread teardown did not complete".to_owned(),
            })
        });
        if completed.is_ok() {
            self.closed = true;
        }
        completed
    }

    /// Reconciles platform registration with whether Rust has an enabled
    /// handler. Call after changing registrations through [`Self::dispatcher_mut`].
    /// A disable requested during a claimed gesture is deferred until that
    /// gesture receives invoke or cancel.
    pub fn synchronize_enabled_state(&mut self) {
        if self.closed
            || self.strategy == AndroidBackStrategy::NativeKey
            || self.bridge.dispatcher().has_active_gesture()
        {
            return;
        }
        let enabled = self.bridge.dispatcher().can_handle();
        let was_registered = self.registered.load(Ordering::Acquire);
        if enabled == was_registered {
            return;
        }
        self.registered.store(enabled, Ordering::Release);
        if enabled {
            install(
                &self.app,
                self.sender.clone(),
                Arc::clone(&self.registration),
                Arc::clone(&self.error),
                Arc::clone(&self.registered),
                self.strategy,
            );
        } else {
            uninstall(
                &self.app,
                Arc::clone(&self.registration),
                Arc::clone(&self.error),
                Arc::clone(&self.registered),
            );
        }
    }

    /// Dispatches queued Java callbacks through the shared Essenty dispatcher.
    /// Returns the number of callbacks consumed.
    pub fn drain_callbacks(&mut self) -> usize {
        let mut count = 0;
        while let Ok(event) = self.receiver.try_recv() {
            count += 1;
            self.callback_count = self.callback_count.saturating_add(1);
            match event {
                PlatformEvent::Started(position, progress) => {
                    self.bridge.handle_gesture_start_with(position);
                    let _ = self.bridge.handle_gesture_progress_with(progress, position);
                }
                PlatformEvent::Progressed(position, progress) => {
                    let _ = self.bridge.handle_gesture_progress_with(progress, position);
                }
                PlatformEvent::Cancelled => {
                    let _ = self.bridge.handle_gesture_cancel();
                }
                PlatformEvent::Invoked if self.bridge.dispatcher().has_active_gesture() => {
                    let _ = self.bridge.handle_gesture_invoke();
                }
                PlatformEvent::Invoked => {
                    self.bridge.handle_back_pressed();
                }
            }
        }
        self.synchronize_enabled_state();
        count
    }

    /// Routes legacy `NativeActivity` input. Modern callback strategies leave
    /// `KEYCODE_BACK` unhandled so Android cannot dispatch the same press twice.
    pub fn handle_legacy_back<F>(
        &mut self,
        event: &android_activity::input::InputEvent<'_>,
        fallback: F,
    ) -> android_activity::InputStatus
    where
        F: FnOnce(&android_activity::input::InputEvent<'_>) -> android_activity::InputStatus,
    {
        use android_activity::input::{InputEvent, KeyAction, Keycode};
        if self.strategy == AndroidBackStrategy::NativeKey {
            if let InputEvent::KeyEvent(key) = event {
                if key.key_code() == Keycode::Back {
                    return match key.action() {
                        KeyAction::Down if self.bridge.dispatcher().can_handle() => {
                            android_activity::InputStatus::Handled
                        }
                        KeyAction::Up if self.bridge.handle_back_pressed() => {
                            android_activity::InputStatus::Handled
                        }
                        _ => fallback(event),
                    };
                }
            }
        }
        fallback(event)
    }
}

impl Drop for AndroidBackHandler {
    fn drop(&mut self) {
        if self.closed || self.strategy == AndroidBackStrategy::NativeKey {
            return;
        }
        if self.bridge.dispatcher().has_active_gesture() {
            if let Ok(mut retries) = UNREGISTER_RETRY.get_or_init(|| Mutex::new(Vec::new())).lock()
            {
                retries.push(Arc::clone(&self.registration));
            }
            return;
        }
        uninstall(
            &self.app,
            Arc::clone(&self.registration),
            Arc::clone(&self.error),
            Arc::clone(&self.registered),
        );
    }
}

#[allow(clippy::too_many_lines)]
fn install(
    app: &AndroidApp,
    sender: mpsc::Sender<PlatformEvent>,
    registration: Arc<Mutex<Option<Registration>>>,
    error: Arc<Mutex<Option<AndroidBackError>>>,
    registered: Arc<AtomicBool>,
    strategy: AndroidBackStrategy,
) {
    let java_app = app.clone();
    let waker = app.create_waker();
    app.run_on_java_main_thread(Box::new(move || {
        // SAFETY: `android-activity` owns the process JavaVM and exposes its
        // valid pointer for the lifetime of this Java-main-thread callback.
        let vm = unsafe { JavaVM::from_raw(java_app.vm_as_ptr().cast()) };
        let setup = vm.attach_current_thread(|env| {
            let raw_activity = java_app.activity_as_ptr() as jni::sys::jobject;
            // SAFETY: `activity_as_ptr` is valid while this queued callback is
            // executing on the Java main thread; the local wrapper never escapes.
            let activity = unsafe { env.as_cast_raw::<Global<JObject<'_>>>(&raw_activity)? };
            retry_quarantined(env);
            let dispatcher = env
                .call_method(
                    activity,
                    jni_str!("getOnBackInvokedDispatcher"),
                    RuntimeMethodSignature::from_str("()Landroid/window/OnBackInvokedDispatcher;")?
                        .method_signature(),
                    &[],
                )?
                .l()?;
            let dispatcher = env.new_global_ref(dispatcher)?;
            let interface = match strategy {
                AndroidBackStrategy::PredictiveCallback => {
                    jni_str!("android.window.OnBackAnimationCallback")
                }
                AndroidBackStrategy::InvokedCallback => {
                    jni_str!("android.window.OnBackInvokedCallback")
                }
                AndroidBackStrategy::NativeKey => return Ok(()),
            };
            let callback_sender = sender.clone();
            let callback_waker = waker.clone();
            let callback_app = java_app.clone();
            let callback_registration = Arc::clone(&registration);
            let callback_error = Arc::clone(&error);
            let callback_registered = Arc::clone(&registered);
            let proxy = DynamicProxy::build(
                env,
                &LoaderContext::None,
                [interface],
                move |env, method, args| {
                    let callback = catch_unwind(AssertUnwindSafe(
                        || -> Result<JObject<'_>, jni::errors::Error> {
                            let name = method.get_name(env)?.to_string();
                            let event = match name.as_str() {
                                "onBackStarted" | "onBackProgressed" => {
                                    let arg = args.get_element(env, 0)?;
                                    let progress = env
                                        .call_method(
                                            &arg,
                                            jni_str!("getProgress"),
                                            jni_sig!(() -> f32),
                                            &[],
                                        )?
                                        .f()?;
                                    let edge = env
                                        .call_method(
                                            &arg,
                                            jni_str!("getSwipeEdge"),
                                            jni_sig!(() -> i32),
                                            &[],
                                        )?
                                        .i()?;
                                    let x = env
                                        .call_method(
                                            &arg,
                                            jni_str!("getTouchX"),
                                            jni_sig!(() -> f32),
                                            &[],
                                        )?
                                        .f()?;
                                    let y = env
                                        .call_method(
                                            &arg,
                                            jni_str!("getTouchY"),
                                            jni_sig!(() -> f32),
                                            &[],
                                        )?
                                        .f()?;
                                    let position = GesturePosition {
                                        swipe_edge: match edge {
                                            0 => SwipeEdge::Left,
                                            1 => SwipeEdge::Right,
                                            _ => SwipeEdge::Unknown,
                                        },
                                        touch_x: x,
                                        touch_y: y,
                                    };
                                    if name == "onBackStarted" {
                                        PlatformEvent::Started(position, progress)
                                    } else {
                                        PlatformEvent::Progressed(position, progress)
                                    }
                                }
                                "onBackCancelled" => PlatformEvent::Cancelled,
                                "onBackInvoked" => PlatformEvent::Invoked,
                                _ => {
                                    log::warn!("unexpected Android back proxy method: {name}");
                                    return Ok(JObject::null());
                                }
                            };
                            let is_terminal =
                                matches!(event, PlatformEvent::Cancelled | PlatformEvent::Invoked);
                            if callback_sender.send(event).is_err() && is_terminal {
                                // The Rust receiver was dropped during teardown.
                                // Keep the proxy alive through the platform's
                                // terminal gesture callback, then unregister.
                                uninstall(
                                    &callback_app,
                                    Arc::clone(&callback_registration),
                                    Arc::clone(&callback_error),
                                    Arc::clone(&callback_registered),
                                );
                            }
                            callback_waker.wake();
                            Ok(JObject::null())
                        },
                    ));
                    match callback {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(cause)) => {
                            if env.exception_check() {
                                env.exception_clear();
                            }
                            log::error!("Android back callback JNI failure: {cause}");
                            Ok(JObject::null())
                        }
                        Err(_) => {
                            log::error!("panic caught in Android back callback");
                            Ok(JObject::null())
                        }
                    }
                },
            )?;
            let signature =
                RuntimeMethodSignature::from_str("(ILandroid/window/OnBackInvokedCallback;)V")?;
            env.call_method(
                &dispatcher,
                jni_str!("registerOnBackInvokedCallback"),
                signature.method_signature(),
                &[JValue::Int(0), JValue::Object(&proxy)],
            )?;
            let mut guard = registration
                .lock()
                .map_err(|_| jni::errors::Error::NullPtr("back registration mutex poisoned"))?;
            *guard = Some(Registration { dispatcher, proxy });
            registered.store(true, Ordering::Release);
            Ok::<_, jni::errors::Error>(())
        });
        if let Err(cause) = setup {
            registered.store(false, Ordering::Release);
            clear_pending_java_exception();
            if let Ok(mut slot) = error.lock() {
                *slot = Some(AndroidBackError { operation: "register", detail: cause.to_string() });
            }
        }
    }));
}

fn uninstall(
    app: &AndroidApp,
    registration: Arc<Mutex<Option<Registration>>>,
    error: Arc<Mutex<Option<AndroidBackError>>>,
    registered: Arc<AtomicBool>,
) {
    app.run_on_java_main_thread(Box::new(move || {
        let result = jni_min_helper::jni_with_env(|env| {
            let mut guard = registration
                .lock()
                .map_err(|_| jni::errors::Error::NullPtr("back registration mutex poisoned"))?;
            if let Some(value) = guard.as_ref() {
                value.unregister(env)?;
                guard.take();
            }
            Ok::<_, jni::errors::Error>(())
        });
        match result {
            Ok(()) => {
                registered.store(false, Ordering::Release);
                remove_quarantined(&registration);
            }
            Err(cause) => {
                registered.store(true, Ordering::Release);
                clear_pending_java_exception();
                if let Ok(mut retries) =
                    UNREGISTER_RETRY.get_or_init(|| Mutex::new(Vec::new())).lock()
                {
                    // Keep the Java proxy alive if Android rejected unregister.
                    // A later host registration retries cleanup on the UI thread.
                    retries.push(Arc::clone(&registration));
                }
                if let Ok(mut slot) = error.lock() {
                    *slot = Some(AndroidBackError {
                        operation: "unregister",
                        detail: cause.to_string(),
                    });
                }
            }
        }
    }));
}

fn remove_quarantined(registration: &Arc<Mutex<Option<Registration>>>) {
    let Some(retries) = UNREGISTER_RETRY.get() else { return };
    if let Ok(mut retries) = retries.lock() {
        retries.retain(|pending| !Arc::ptr_eq(pending, registration));
    }
}

fn clear_pending_java_exception() {
    let _ = jni_min_helper::jni_with_env(|env| {
        if env.exception_check() {
            env.exception_clear();
        }
        Ok::<_, jni::errors::Error>(())
    });
}

fn retry_quarantined(env: &mut Env<'_>) {
    let Some(retry) = UNREGISTER_RETRY.get() else { return };
    let Ok(mut retry) = retry.lock() else { return };
    let mut retained = Vec::new();
    for registration in retry.drain(..) {
        let Ok(mut slot) = registration.lock() else {
            retained.push(registration);
            continue;
        };
        let Some(value) = slot.as_ref() else { continue };
        if value.unregister(env).is_ok() {
            slot.take();
        } else {
            if env.exception_check() {
                env.exception_clear();
            }
            retained.push(Arc::clone(&registration));
        }
    }
    *retry = retained;
}
