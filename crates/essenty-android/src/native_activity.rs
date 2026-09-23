//! Rust `NativeActivity` event-loop integration.

use std::time::Duration;

use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, Keycode},
};
use essenty_back_handler::BackDispatcher;
use essenty_lifecycle::{LifecycleRegistry, LifecycleState};

/// Drives Essenty lifecycle from `android-activity` events while preserving
/// the host's access to every polled event.
///
/// Construct this in `android_main`. Call [`Self::poll_events`] wherever the
/// Rust application would normally poll `AndroidApp`. The host does not
/// manually forward `onStart`, `onResume`, or other Java callbacks.
#[derive(Debug)]
pub struct NativeActivityLifecycle {
    app: AndroidApp,
    registry: LifecycleRegistry,
}

impl NativeActivityLifecycle {
    /// Wraps a Rust `NativeActivity` host after Android has called `onCreate`.
    #[must_use]
    pub fn new(app: AndroidApp) -> Self {
        let registry = LifecycleRegistry::new();
        let _ = registry.move_to(LifecycleState::Created);
        Self { app, registry }
    }

    /// The shared Rust lifecycle registry.
    #[must_use]
    pub fn registry(&self) -> &LifecycleRegistry {
        &self.registry
    }

    /// The Android app for window and input APIs.
    #[must_use]
    pub fn app(&self) -> &AndroidApp {
        &self.app
    }

    /// Polls native events, updating Essenty before calling `callback`.
    ///
    /// Must be called from the `android_main` thread, as required by
    /// `AndroidApp::poll_events`.
    pub fn poll_events<F>(&self, timeout: Option<Duration>, mut callback: F)
    where
        F: FnMut(PollEvent<'_>),
    {
        self.app.poll_events(timeout, |event| {
            if let PollEvent::Main(main) = &event {
                let target = match main {
                    MainEvent::Start | MainEvent::Pause => Some(LifecycleState::Started),
                    MainEvent::Resume { .. } => Some(LifecycleState::Resumed),
                    MainEvent::Stop => Some(LifecycleState::Created),
                    MainEvent::Destroy => Some(LifecycleState::Destroyed),
                    _ => None,
                };
                if let Some(target) = target {
                    let _ = self.registry.move_to(target);
                }
            }
            callback(event);
        });
    }

    /// Drains native input, routing the ordinary Android Back key to Essenty.
    /// Other events go to `fallback` unchanged. Call this when
    /// `MainEvent::InputAvailable` arrives.
    ///
    /// # Errors
    /// Propagates an `android-activity` input error.
    pub fn handle_input_events<F>(
        &self,
        back: &mut BackDispatcher,
        mut fallback: F,
    ) -> android_activity::error::Result<()>
    where
        F: FnMut(&InputEvent<'_>) -> InputStatus,
    {
        let mut input = self.app.input_events_iter()?;
        while input.next(|event| match event {
            InputEvent::KeyEvent(key) if key.key_code() == Keycode::Back => match key.action() {
                KeyAction::Down if back.can_handle() => InputStatus::Handled,
                KeyAction::Up if back.back() => InputStatus::Handled,
                _ => fallback(event),
            },
            _ => fallback(event),
        }) {}
        Ok(())
    }
}
