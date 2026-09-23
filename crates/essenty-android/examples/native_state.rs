//! Compile-checked `NativeActivity` state handoff. The host drives these event
//! handlers from `NativeActivityLifecycle::poll_events`.

#[cfg(target_os = "android")]
use android_activity::{MainEvent, PollEvent};
#[cfg(target_os = "android")]
use essenty_android::{NativeActivityLifecycle, NativeActivityState};

#[cfg(target_os = "android")]
/// Processes one native state event.
///
/// # Panics
/// Panics if saved state is malformed, a provider fails, or no keeper exists
/// when Android requests a save. Production hosts should handle these errors.
pub fn handle_event<'a>(
    event: PollEvent<'a>,
    state: &mut Option<NativeActivityState>,
    snapshot: &'a mut Vec<u8>,
) {
    match event {
        PollEvent::Main(MainEvent::Resume { loader, .. }) => {
            if state.is_none() {
                *state = Some(NativeActivityState::from_loader(&loader).unwrap());
            }
        }
        PollEvent::Main(MainEvent::SaveState { saver, .. }) => {
            *snapshot = state.as_ref().unwrap().save_bytes().unwrap();
            NativeActivityState::store(&saver, snapshot);
        }
        _ => {}
    }
}

#[cfg(target_os = "android")]
/// Polls once and forwards state events.
///
/// # Panics
/// Panics under the conditions described by [`handle_event`].
pub fn poll_once(
    host: &NativeActivityLifecycle,
    state: &mut Option<NativeActivityState>,
    snapshot: &mut Vec<u8>,
) {
    host.poll_events(None, |event| handle_event(event, state, snapshot));
}

fn main() {}
