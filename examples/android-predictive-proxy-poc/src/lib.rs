use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use essenty_back_handler::{BackDispatcher, BackPhase, GesturePosition, SwipeEdge};
use jni::{
    Env, JavaVM,
    errors::Error,
    jni_sig, jni_str,
    objects::{JObject, JValue},
    refs::{Global, LoaderContext},
    signature::RuntimeMethodSignature,
};
use jni_min_helper::DynamicProxy;

#[derive(Debug)]
enum PlatformEvent {
    Started { progress: f32, edge: i32, x: f32, y: f32 },
    Progressed { progress: f32, edge: i32, x: f32, y: f32 },
    Cancelled,
    Invoked,
}

struct Registration {
    dispatcher: Global<JObject<'static>>,
    proxy: DynamicProxy,
}

impl Registration {
    fn unregister(&self, env: &mut Env<'_>) -> Result<(), Error> {
        let sig = RuntimeMethodSignature::from_str("(Landroid/window/OnBackInvokedCallback;)V")?;
        env.call_method(
            &self.dispatcher,
            jni_str!("unregisterOnBackInvokedCallback"),
            sig.method_signature(),
            &[JValue::Object(&self.proxy)],
        )?;
        log::info!("POC UNREGISTERED proxy={}", self.proxy.id());
        Ok(())
    }
}

fn back_values<'a>(env: &mut Env<'a>, event: &JObject<'a>) -> Result<(f32, i32, f32, f32), Error> {
    let progress =
        env.call_method(event, jni_str!("getProgress"), jni_sig!(() -> f32), &[])?.f()?;
    let edge = env.call_method(event, jni_str!("getSwipeEdge"), jni_sig!(() -> i32), &[])?.i()?;
    let x = env.call_method(event, jni_str!("getTouchX"), jni_sig!(() -> f32), &[])?.f()?;
    let y = env.call_method(event, jni_str!("getTouchY"), jni_sig!(() -> f32), &[])?.f()?;
    Ok((progress, edge, x, y))
}

fn install(
    app: &AndroidApp,
    events: mpsc::Sender<PlatformEvent>,
    registration: Arc<Mutex<Option<Registration>>>,
) {
    let app = app.clone();
    let waker = app.create_waker();
    let java_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(java_app.vm_as_ptr().cast()) };
        let result = vm.attach_current_thread(|env| {
            let raw_activity = java_app.activity_as_ptr() as jni::sys::jobject;
            let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
            let sdk = jni_min_helper::android_api_level();
            if sdk < 33 {
                log::info!("POC LEGACY_NATIVE_ACTIVITY_BACK sdk={sdk}");
                return Ok(());
            }
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
            let interface = if sdk >= 34 && !cfg!(feature = "invoke-only") {
                jni_str!("android.window.OnBackAnimationCallback")
            } else {
                jni_str!("android.window.OnBackInvokedCallback")
            };
            let proxy = DynamicProxy::build(
                env,
                &LoaderContext::None,
                &[interface],
                move |env, method, args| {
                    let name = method.get_name(env)?.to_string();
                    let event = match name.as_str() {
                        "onBackStarted" | "onBackProgressed" => {
                            let arg = args.get_element(env, 0)?;
                            let (progress, edge, x, y) = back_values(env, &arg)?;
                            if name == "onBackStarted" {
                                PlatformEvent::Started { progress, edge, x, y }
                            } else {
                                PlatformEvent::Progressed { progress, edge, x, y }
                            }
                        }
                        "onBackCancelled" => PlatformEvent::Cancelled,
                        "onBackInvoked" => PlatformEvent::Invoked,
                        _ => {
                            log::warn!("POC UNKNOWN_METHOD {name}");
                            return Ok(JObject::null());
                        }
                    };
                    log::info!("POC JNI_CALLBACK {event:?}");
                    let _ = events.send(event);
                    waker.wake();
                    Ok(JObject::null())
                },
            )?;
            let sig =
                RuntimeMethodSignature::from_str("(ILandroid/window/OnBackInvokedCallback;)V")?;
            env.call_method(
                &dispatcher,
                jni_str!("registerOnBackInvokedCallback"),
                sig.method_signature(),
                &[JValue::Int(0), JValue::Object(&proxy)],
            )?;
            log::info!("POC REGISTERED sdk={sdk} proxy={}", proxy.id());
            *registration.lock().unwrap() = Some(Registration { dispatcher, proxy });
            Ok::<_, Error>(())
        });
        if let Err(err) = result {
            log::error!("POC REGISTER_ERROR {err:?}");
        }
    }));
}

fn uninstall(app: &AndroidApp, registration: Arc<Mutex<Option<Registration>>>) {
    app.run_on_java_main_thread(Box::new(move || {
        let result = jni_min_helper::jni_with_env(|env| {
            let mut guard = registration.lock().unwrap();
            if let Some(value) = guard.as_ref() {
                value.unregister(env)?;
                drop(guard.take());
                log::info!("POC PROXY_DROPPED");
            }
            Ok(())
        });
        if let Err(err) = result {
            log::error!("POC UNREGISTER_ERROR {err:?}");
        }
    }));
}

fn inject_synthetic(app: &AndroidApp, registration: Arc<Mutex<Option<Registration>>>) {
    app.run_on_java_main_thread(Box::new(move || {
        let result = jni_min_helper::jni_with_env(|env| {
            if jni_min_helper::android_api_level() < 34 {
                return Ok(());
            }
            let guard = registration.lock().unwrap();
            let Some(value) = guard.as_ref() else {
                return Ok(());
            };
            let event_sig = RuntimeMethodSignature::from_str("(FFFI)V")?;
            let callback_sig = RuntimeMethodSignature::from_str("(Landroid/window/BackEvent;)V")?;
            log::info!("POC SYNTHETIC_BEGIN");
            for (name, progress, x, y, edge) in [
                ("onBackStarted", 0.0, 12.0, 400.0, 0),
                ("onBackProgressed", 0.15, 75.0, 410.0, 0),
                ("onBackProgressed", 0.67, 350.0, 420.0, 0),
                ("onBackProgressed", 0.92, 700.0, 430.0, 1),
            ] {
                let back_event = env.new_object(
                    jni_str!("android/window/BackEvent"),
                    event_sig.method_signature(),
                    &[
                        JValue::Float(x),
                        JValue::Float(y),
                        JValue::Float(progress),
                        JValue::Int(edge),
                    ],
                )?;
                let method_name = jni::strings::JNIString::new(name);
                env.call_method(
                    &value.proxy,
                    &method_name,
                    callback_sig.method_signature(),
                    &[JValue::Object(&back_event)],
                )?;
            }
            env.call_method(&value.proxy, jni_str!("onBackCancelled"), jni_sig!(() -> ()), &[])?;
            log::info!("POC SYNTHETIC_END");
            Ok(())
        });
        if let Err(err) = result {
            log::error!("POC SYNTHETIC_ERROR {err:?}");
        }
    }));
}

fn position(edge: i32, x: f32, y: f32) -> GesturePosition {
    GesturePosition {
        swipe_edge: match edge {
            0 => SwipeEdge::Left,
            1 => SwipeEdge::Right,
            _ => SwipeEdge::Unknown,
        },
        touch_x: x,
        touch_y: y,
    }
}

fn deliver(dispatcher: &mut BackDispatcher, event: PlatformEvent) {
    match event {
        PlatformEvent::Started { progress, edge, x, y } => {
            log::info!("POC PLATFORM_START progress={progress} edge={edge} x={x} y={y}");
            dispatcher.predictive_start_with(position(edge, x, y));
            let _ = dispatcher.predictive_progress_with(progress, position(edge, x, y));
        }
        PlatformEvent::Progressed { progress, edge, x, y } => {
            let _ = dispatcher.predictive_progress_with(progress, position(edge, x, y));
        }
        PlatformEvent::Cancelled => {
            let _ = dispatcher.predictive_cancel();
        }
        PlatformEvent::Invoked => {
            if dispatcher.has_active_gesture() {
                let _ = dispatcher.predictive_invoke();
            } else {
                dispatcher.back();
            }
        }
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag(b"EssentyProxyPOC"),
    );
    let (tx, rx) = mpsc::channel();
    let registration = Arc::new(Mutex::new(None));
    let mut dispatcher = BackDispatcher::new();
    dispatcher.register(0, true, |event| match event.phase {
        BackPhase::Started | BackPhase::Progressed | BackPhase::Cancelled | BackPhase::Invoked => {
            log::info!("POC ESSENTY_EVENT {event:?}")
        }
    });
    install(&app, tx, Arc::clone(&registration));
    let started = Instant::now();
    let mut uninstalled = false;
    let mut manual_uninstall = false;
    let mut synthetic_requested = false;
    let mut legacy_requested = false;
    let mut destroy = false;
    while !destroy {
        app.poll_events(Some(Duration::from_millis(50)), |event| match event {
            PollEvent::Main(MainEvent::InputAvailable) => {
                if let Ok(mut input) = app.input_events_iter() {
                    while input.next(|event| {
                        if let InputEvent::KeyEvent(key) = event {
                            if key.key_code() == Keycode::Space && key.action() == KeyAction::Up {
                                manual_uninstall = true;
                                return InputStatus::Handled;
                            }
                            if key.key_code() == Keycode::S && key.action() == KeyAction::Up {
                                synthetic_requested = true;
                                return InputStatus::Handled;
                            }
                            if key.key_code() == Keycode::Back
                                && key.action() == KeyAction::Up
                                && jni_min_helper::android_api_level() < 33
                            {
                                legacy_requested = true;
                                return InputStatus::Handled;
                            }
                        }
                        InputStatus::Unhandled
                    }) {}
                }
            }
            PollEvent::Main(MainEvent::Destroy) => destroy = true,
            _ => {}
        });
        while let Ok(event) = rx.try_recv() {
            deliver(&mut dispatcher, event);
        }
        if legacy_requested {
            dispatcher.back();
            legacy_requested = false;
        }
        if synthetic_requested {
            inject_synthetic(&app, Arc::clone(&registration));
            synthetic_requested = false;
        }
        if !uninstalled && (manual_uninstall || started.elapsed() >= Duration::from_secs(600)) {
            uninstall(&app, Arc::clone(&registration));
            uninstalled = true;
            log::info!("POC UNREGISTER_REQUESTED");
        }
    }
    if !uninstalled {
        uninstall(&app, Arc::clone(&registration));
        log::info!("POC DESTROY_UNREGISTER_REQUESTED");
    }
    log::info!("POC DESTROY");
}
