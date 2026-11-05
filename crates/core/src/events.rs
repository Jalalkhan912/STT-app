/// Events driving the dictation controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Pressed { at_ms: u64 },
    Released { at_ms: u64 },
    Cancel,
    MaxDuration,
}
