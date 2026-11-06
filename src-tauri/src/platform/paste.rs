//! Paste via clipboard plus a synthetic Cmd/Ctrl+V, restoring the previous
//! clipboard text afterwards. Neither the transcript nor clipboard contents
//! are ever logged.
use super::focus;
use arboard::Clipboard;
use dictation_core::controller::{PasteOutcome, Paster};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// How long the target app gets to read the clipboard before we restore it.
const RESTORE_DELAY: Duration = Duration::from_millis(300);
const KEY_GAP: Duration = Duration::from_millis(20);

#[cfg(target_os = "macos")]
const MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MODIFIER: Key = Key::Control;

// Physical-key codes so the chord works on any keyboard layout:
// kVK_ANSI_V on macOS, VK_V on Windows.
#[cfg(target_os = "macos")]
const V_KEY: Key = Key::Other(0x09);
#[cfg(not(target_os = "macos"))]
const V_KEY: Key = Key::Other(0x56);

pub struct SystemPaster;

impl SystemPaster {
    pub fn new() -> Self {
        SystemPaster
    }
}

impl Default for SystemPaster {
    fn default() -> Self {
        Self::new()
    }
}

fn send_paste_chord() -> Result<(), String> {
    let settings = Settings {
        // Permissions are handled by our own onboarding, not by a surprise prompt.
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    };
    let mut enigo = Enigo::new(&settings).map_err(|e| e.to_string())?;
    enigo
        .key(MODIFIER, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(KEY_GAP);
    let click = enigo.key(V_KEY, Direction::Click);
    std::thread::sleep(KEY_GAP);
    // Always release the modifier, even if the click failed.
    let release = enigo.key(MODIFIER, Direction::Release);
    click.map_err(|e| e.to_string())?;
    release.map_err(|e| e.to_string())
}

impl Paster for SystemPaster {
    fn focused_app(&self) -> Option<String> {
        focus::frontmost_app()
    }

    fn paste(&mut self, text: &str) -> PasteOutcome {
        let Ok(mut clipboard) = Clipboard::new() else {
            return PasteOutcome::CopiedOnly;
        };
        let prev = clipboard.get_text().ok();
        if clipboard.set_text(text.to_string()).is_err() {
            return PasteOutcome::CopiedOnly;
        }
        if focus::target_blocks_input() || send_paste_chord().is_err() {
            return PasteOutcome::CopiedOnly;
        }
        if let Some(prev) = prev {
            let ours = text.to_string();
            std::thread::spawn(move || {
                std::thread::sleep(RESTORE_DELAY);
                if let Ok(mut cb) = Clipboard::new() {
                    if cb.get_text().is_ok_and(|now| now == ours) {
                        let _ = cb.set_text(prev);
                    }
                }
            });
        }
        PasteOutcome::Pasted
    }
}
