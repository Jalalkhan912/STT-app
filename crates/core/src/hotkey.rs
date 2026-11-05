use std::collections::HashSet;

use crate::{events::Event, settings::HotkeySpec};

/// Tracks held keys (W3C `KeyboardEvent.code` strings) and turns them into
/// push-to-talk events.
pub struct HotkeyTracker {
    spec: HotkeySpec,
    held: HashSet<String>,
    active: bool,
    suppressed: bool,
}

impl HotkeyTracker {
    pub fn new(spec: HotkeySpec) -> Self {
        Self {
            spec,
            held: HashSet::new(),
            active: false,
            suppressed: false,
        }
    }

    /// Replaces the hotkey and clears all tracked state.
    pub fn set_spec(&mut self, spec: HotkeySpec) {
        self.spec = spec;
        self.held.clear();
        self.active = false;
        self.suppressed = false;
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
