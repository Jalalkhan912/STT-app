//! Placeholder overlay until Task 12 provides the real one.
use dictation_core::controller::{Overlay, OverlayState};

pub struct StubOverlay;

impl Overlay for StubOverlay {
    fn set(&self, s: OverlayState) {
        log::info!("overlay: {s:?}");
    }
}
