//! Maps `rdev` keys to W3C `KeyboardEvent.code` names (what the UI stores in
//! `HotkeySpec`). Deliberately free of platform cfgs so tests run everywhere.
use rdev::Key;

pub fn code(k: Key) -> Option<&'static str> {
    Some(match k {
        Key::Alt => "AltLeft",
        // rdev reports the right Option key on macOS (and AltGr elsewhere) as AltGr.
        Key::AltGr => "AltRight",
        Key::ControlLeft => "ControlLeft",
        Key::ControlRight => "ControlRight",
        Key::ShiftLeft => "ShiftLeft",
        Key::ShiftRight => "ShiftRight",
        Key::MetaLeft => "MetaLeft",
        Key::MetaRight => "MetaRight",
        Key::KeyA => "KeyA",
        Key::KeyB => "KeyB",
        Key::KeyC => "KeyC",
        Key::KeyD => "KeyD",
        Key::KeyE => "KeyE",
        Key::KeyF => "KeyF",
        Key::KeyG => "KeyG",
        Key::KeyH => "KeyH",
        Key::KeyI => "KeyI",
        Key::KeyJ => "KeyJ",
        Key::KeyK => "KeyK",
        Key::KeyL => "KeyL",
        Key::KeyM => "KeyM",
        Key::KeyN => "KeyN",
        Key::KeyO => "KeyO",
        Key::KeyP => "KeyP",
        Key::KeyQ => "KeyQ",
        Key::KeyR => "KeyR",
        Key::KeyS => "KeyS",
        Key::KeyT => "KeyT",
        Key::KeyU => "KeyU",
        Key::KeyV => "KeyV",
        Key::KeyW => "KeyW",
        Key::KeyX => "KeyX",
        Key::KeyY => "KeyY",
        Key::KeyZ => "KeyZ",
        Key::Num0 => "Digit0",
        Key::Num1 => "Digit1",
        Key::Num2 => "Digit2",
        Key::Num3 => "Digit3",
        Key::Num4 => "Digit4",
        Key::Num5 => "Digit5",
        Key::Num6 => "Digit6",
        Key::Num7 => "Digit7",
        Key::Num8 => "Digit8",
        Key::Num9 => "Digit9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::Space => "Space",
        Key::Escape => "Escape",
        Key::Tab => "Tab",
        Key::CapsLock => "CapsLock",
        Key::LeftArrow => "ArrowLeft",
        Key::RightArrow => "ArrowRight",
        Key::UpArrow => "ArrowUp",
        Key::DownArrow => "ArrowDown",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_modifiers() {
        assert_eq!(code(Key::ControlRight), Some("ControlRight"));
        assert_eq!(code(Key::ShiftLeft), Some("ShiftLeft"));
        assert_eq!(code(Key::Escape), Some("Escape"));
        assert_eq!(code(Key::KeyD), Some("KeyD"));
        assert_eq!(code(Key::Space), Some("Space"));
    }

    #[test]
    fn keymap_altgr_is_alt_right() {
        assert_eq!(code(Key::AltGr), Some("AltRight"));
    }

    #[test]
    fn keymap_unmapped_is_none() {
        assert_eq!(code(Key::PrintScreen), None);
    }
}
