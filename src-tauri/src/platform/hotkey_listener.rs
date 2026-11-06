//! Global push-to-talk hotkey via `rdev`.
//!
//! The rdev callback runs inside an OS event hook (a CGEventTap on the main
//! run loop on macOS, a low-level hook on Windows). The OS disables hooks that
//! are slow, so the callback only feeds the tracker and sends on channels.
use super::keymap;
use crate::state::now_ms;
use dictation_core::events::Event;
use dictation_core::hotkey::HotkeyTracker;
use dictation_core::settings::HotkeySpec;
use rdev::EventType;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Starts the listener thread. `on_pressed` runs on the hook thread just
/// before `Event::Pressed` is forwarded, so it must return immediately.
pub fn spawn(
    spec: Arc<RwLock<HotkeySpec>>,
    tx: Sender<Event>,
    on_pressed: impl Fn() + Send + Sync + 'static,
) {
    // Only keyboard events are needed; this also keeps the macOS tap light
    // (the rdev fork reads this variable when it builds the event mask).
    #[cfg(target_os = "macos")]
    std::env::set_var("KEYBOARD_ONLY", "1");

    let on_pressed = Arc::new(on_pressed);
    let spawned = std::thread::Builder::new()
        .name("hotkey".into())
        .spawn(move || loop {
            let result = rdev::listen(make_callback(spec.clone(), tx.clone(), on_pressed.clone()));
            match result {
                // On macOS the tap is attached to the main run loop and
                // `listen` returns immediately; events keep flowing.
                Ok(()) => {
                    log::info!("hotkey listener installed");
                    return;
                }
                Err(e) => {
                    // Typically missing Input Monitoring / Accessibility
                    // permission. Retry so granting it takes effect.
                    log::warn!("hotkey listener failed: {e:?}; retrying in 3s");
                    std::thread::sleep(Duration::from_secs(3));
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("cannot start hotkey thread: {e}");
    }
}

fn make_callback(
    spec: Arc<RwLock<HotkeySpec>>,
    tx: Sender<Event>,
    on_pressed: Arc<impl Fn() + Send + Sync + 'static>,
) -> impl FnMut(rdev::Event) + 'static {
    let mut current = spec.read().unwrap_or_else(|e| e.into_inner()).clone();
    let mut tracker = HotkeyTracker::new(current.clone());
    move |event| {
        let (key, down) = match event.event_type {
            EventType::KeyPress(k) => (k, true),
            EventType::KeyRelease(k) => (k, false),
            _ => return,
        };
        let Some(code) = keymap::code(key) else {
            return;
        };
        // Cheap read; a changed hotkey replaces the tracker state.
        if let Ok(latest) = spec.try_read() {
            if latest.keys != current.keys {
                current = latest.clone();
                tracker.set_spec(current.clone());
            }
        }
        if let Some(ev) = tracker.on_key(code, down, now_ms().max(0) as u64) {
            if matches!(ev, Event::Pressed { .. }) {
                on_pressed();
            }
            let _ = tx.send(ev);
        }
    }
}
