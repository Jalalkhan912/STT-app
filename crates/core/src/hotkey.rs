use std::collections::HashSet;

use crate::{controller::MIN_HOLD_MS, events::Event, settings::HotkeySpec};

/// Tracks held keys (W3C `KeyboardEvent.code` strings) and turns them into
/// push-to-talk events.
pub struct HotkeyTracker {
    spec: HotkeySpec,
    held: HashSet<String>,
    active: bool,
    pressed_at: u64,
    suppressed: bool,
}

impl HotkeyTracker {
    pub fn new(spec: HotkeySpec) -> Self {
        Self {
            spec,
            held: HashSet::new(),
            active: false,
            pressed_at: 0,
            suppressed: false,
        }
    }

    /// Replaces the hotkey and clears all tracked state. If the old hotkey was
    /// held, ends that hold: `Cancel` for a hold shorter than the minimum
    /// (an accidental tap), otherwise `Released`.
    pub fn set_spec(&mut self, spec: HotkeySpec, at_ms: u64) -> Option<Event> {
        let was_active = self.active;
        let held_ms = at_ms.saturating_sub(self.pressed_at);
        self.spec = spec;
        self.held.clear();
        self.active = false;
        self.suppressed = false;
        if !was_active {
            None
        } else if held_ms < MIN_HOLD_MS {
            Some(Event::Cancel)
        } else {
            Some(Event::Released { at_ms })
        }
    }

    pub fn on_key(&mut self, code: &str, down: bool, at_ms: u64) -> Option<Event> {
        let in_spec = self.spec.keys.iter().any(|k| k == code);
        if down {
            if code == "Escape" && self.active {
                self.active = false;
                self.suppressed = true;
                return Some(Event::Cancel);
            }
            self.held.insert(code.to_string());
            if in_spec && !self.active && !self.suppressed && self.all_held() {
                self.active = true;
                self.pressed_at = at_ms;
                return Some(Event::Pressed { at_ms });
            }
            None
        } else {
            self.held.remove(code);
            if self.suppressed && !self.spec.keys.iter().any(|k| self.held.contains(k)) {
                self.suppressed = false;
            }
            if in_spec && self.active {
                self.active = false;
                return Some(Event::Released { at_ms });
            }
            None
        }
    }

    fn all_held(&self) -> bool {
        !self.spec.keys.is_empty() && self.spec.keys.iter().all(|k| self.held.contains(k))
    }
}
