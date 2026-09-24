use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use essenty_android::{AndroidBackHandler, NativeActivityLifecycle, NativeActivityState};
use essenty_back_handler::{BackDispatcher, BackPhase};
use essenty_instance_keeper::InstanceKeeper;
use jni::{
    JavaVM, jni_sig, jni_str,
    objects::{Global, JObject, JString},
};

static ANDROID_MAIN_CALLS: AtomicUsize = AtomicUsize::new(0);
static NEXT_KEEPER_ID: AtomicUsize = AtomicUsize::new(1);
static RETAINED_DROPS: AtomicUsize = AtomicUsize::new(0);
static BACK_ATTACHES: AtomicUsize = AtomicUsize::new(0);
static BACK_INVOKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn finish_activity(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
            let raw_activity = callback_app.activity_as_ptr() as jni::sys::jobject;
            let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
            env.call_method(activity.as_ref(), jni_str!("finish"), jni_sig!("()V"), &[])?;
            Ok(())
        });
    }));
}

fn java_resource_configuration(app: &AndroidApp) -> String {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| {
        let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw_activity)? };
        let resources = env
            .call_method(
                activity.as_ref(),
                jni_str!("getResources"),
                jni_sig!("()Landroid/content/res/Resources;"),
                &[],
            )?
            .l()?;
        let resources = JObject::from(resources);
        let configuration = env
            .call_method(
                &resources,
                jni_str!("getConfiguration"),
                jni_sig!("()Landroid/content/res/Configuration;"),
                &[],
            )?
            .l()?;
        let configuration = JObject::from(configuration);
        let orientation = env
            .get_field(&configuration, jni_str!("orientation"), jni_sig!("I"))?
            .i()?;
        let ui_mode = env
            .get_field(&configuration, jni_str!("uiMode"), jni_sig!("I"))?
            .i()?;
        let screen_width_dp = env
            .get_field(&configuration, jni_str!("screenWidthDp"), jni_sig!("I"))?
            .i()?;
        let screen_height_dp = env
            .get_field(&configuration, jni_str!("screenHeightDp"), jni_sig!("I"))?
            .i()?;
        let density_dpi = env
            .get_field(&configuration, jni_str!("densityDpi"), jni_sig!("I"))?
            .i()?;
        let font_scale = env
            .get_field(&configuration, jni_str!("fontScale"), jni_sig!("F"))?
            .f()?;
        let rendered = env
            .call_method(
                &configuration,
                jni_str!("toString"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let rendered = unsafe { env.as_cast_raw::<JString>(&rendered)? };
        let rendered = rendered.try_to_string(env)?;
        Ok::<_, jni::errors::Error>(format!(
            "{rendered} fields[orientation={orientation},uiMode=0x{ui_mode:x},window_dp={screen_width_dp}x{screen_height_dp},density_dpi={density_dpi},font_scale={font_scale}]"
        ))
    })
    .expect("read current Activity Resources configuration")
}

struct RetainedProbe {
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for RetainedProbe {
    fn drop(&mut self) {
        let drops = RETAINED_DROPS.fetch_add(1, Ordering::SeqCst) + 1;
        log::info!("PROOF retained_drop_count={drops}");
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("EssentyIK"),
    );

    let invocation = ANDROID_MAIN_CALLS.fetch_add(1, Ordering::SeqCst) + 1;
    BACK_INVOKED.store(false, Ordering::SeqCst);
    let thread = format!("{:?}", std::thread::current().id());
    let mut keeper = InstanceKeeper::new();
    let keeper_id = NEXT_KEEPER_ID.fetch_add(1, Ordering::SeqCst);
    let keeper_address = (&keeper as *const InstanceKeeper) as usize;
    let retained = keeper
        .get_or_create("runtime-proof", || RetainedProbe { _not_send: PhantomData })
        .expect("new keeper accepts retained probe");
    let retained_address = Rc::as_ptr(&retained) as usize;
    drop(retained);

    log::info!(
        "PROOF android_main_enter invocation={invocation} thread={thread} keeper_id={keeper_id} keeper_address=0x{keeper_address:x} retained_address=0x{retained_address:x} drops={} java_config={}",
        RETAINED_DROPS.load(Ordering::SeqCst),
        java_resource_configuration(&app)
    );

    let host = NativeActivityLifecycle::new(app.clone());
    let lifecycle_callback_count = Rc::new(Cell::new(0_usize));
    let lifecycle_callback_count_observer = Rc::clone(&lifecycle_callback_count);
    let _lifecycle_observer = host.registry().subscribe(move |_| {
        lifecycle_callback_count_observer.set(lifecycle_callback_count_observer.get() + 1);
    });
    let mut back = AndroidBackHandler::attach(&app, BackDispatcher::new());
    let _back_registration = back.dispatcher_mut().register(0, true, |event| {
        log::info!("PROOF integrated_back_event={event:?}");
        if event.phase == BackPhase::Invoked {
            BACK_INVOKED.store(true, Ordering::SeqCst);
        }
    });
    back.synchronize_enabled_state();
    let back_attaches = BACK_ATTACHES.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!("PROOF back_adapter_attach_count={back_attaches}");

    let mut state: Option<NativeActivityState> = None;
    let mut snapshot = Vec::new();
    let mut config_changes = 0_usize;
    let mut destroy_events = 0_usize;
    let mut destroyed = false;
    let mut finish_requested = false;
    let mut back_closed = false;

    while !destroyed {
        host.poll_events(Some(Duration::from_millis(100)), |event| match event {
            PollEvent::Main(MainEvent::InputAvailable) => {
                host.handle_input_events(back.dispatcher_mut(), |input| {
                    if let InputEvent::KeyEvent(key) = input
                        && key.key_code() == Keycode::F12
                        && key.action() == KeyAction::Up
                    {
                        finish_requested = true;
                        InputStatus::Handled
                    } else {
                        InputStatus::Unhandled
                    }
                })
                .expect("read NativeActivity input events");
            }
            PollEvent::Main(MainEvent::Resume { loader, .. }) => {
                if state.is_none() {
                    let mut restored = NativeActivityState::from_loader(&loader)
                        .expect("restore native saved state");
                    let restored_value = restored
                        .keeper_mut()
                        .consume_bytes("runtime-proof");
                    log::info!(
                        "PROOF state_resume invocation={invocation} restored={restored_value:?}"
                    );
                    restored
                        .keeper_mut()
                        .register("runtime-proof", || b"native-state-round-trip".to_vec())
                        .expect("register native state proof provider");
                    state = Some(restored);
                }
            }
            PollEvent::Main(MainEvent::SaveState { saver, .. }) => {
                snapshot = state
                    .as_ref()
                    .expect("state initialized on resume")
                    .save_bytes()
                    .expect("encode native saved state");
                NativeActivityState::store(&saver, &snapshot);
                log::info!(
                    "PROOF state_saved invocation={invocation} envelope_bytes={}",
                    snapshot.len()
                );
            }
            PollEvent::Main(MainEvent::ConfigChanged { .. }) => {
                config_changes += 1;
                let config = app.config();
                let current = keeper
                    .get::<RetainedProbe>("runtime-proof")
                    .expect("keeper remains live")
                    .expect("retained probe remains present");
                let current_address = Rc::as_ptr(&current) as usize;
                log::info!(
                    "PROOF config_changed count={config_changes} invocation={invocation} thread={:?} keeper_id={keeper_id} keeper_address=0x{:x} retained_address=0x{current_address:x} same_retained={} orientation={:?} locale={:?} density={:?} layout_direction={:?} ui_mode_night={:?} screen_width_dp={:?} screen_height_dp={:?} config={:?} java_config={}",
                    std::thread::current().id(),
                    (&keeper as *const InstanceKeeper) as usize,
                    current_address == retained_address,
                    config.orientation(),
                    config.language(),
                    config.density(),
                    config.layout_direction(),
                    config.ui_mode_night(),
                    config.screen_width_dp(),
                    config.screen_height_dp(),
                    config.copy(),
                    java_resource_configuration(&app),
                );
                log::info!(
                    "PROOF lifecycle_config callbacks={} subscribers={}",
                    lifecycle_callback_count.get(),
                    host.registry().subscriber_count()
                );
                drop(current);
            }
            PollEvent::Main(MainEvent::Destroy) => {
                destroy_events += 1;
                log::info!(
                    "PROOF destroy_event count={destroy_events} invocation={invocation} thread={:?} keeper_id={keeper_id} config_changes={config_changes}",
                    std::thread::current().id()
                );
                destroyed = true;
            }
            _ => {}
        });
        back.drain_callbacks();
        if finish_requested {
            log::info!("PROOF explicit_finish_key=F12");
            finish_requested = false;
            back.close().expect("close integrated Android back adapter");
            back_closed = true;
            log::info!("PROOF back_adapter_closed=true");
            finish_activity(app.clone());
        }
        if BACK_INVOKED.swap(false, Ordering::SeqCst) {
            finish_activity(app.clone());
        }
    }

    log::info!("PROOF back_adapter_closed={back_closed}");
    keeper.destroy_all();
    drop(keeper);
    log::info!(
        "PROOF keeper_destroyed invocation={invocation} keeper_id={keeper_id} destroy_events={destroy_events} config_changes={config_changes} back_attach_count={} lifecycle_callbacks={} lifecycle_subscribers={} retained_drop_count={}",
        BACK_ATTACHES.load(Ordering::SeqCst),
        lifecycle_callback_count.get(),
        host.registry().subscriber_count(),
        RETAINED_DROPS.load(Ordering::SeqCst),
    );
}
