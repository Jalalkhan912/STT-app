//! Placeholder controller ports until Tasks 11 and 12 provide real ones.
use dictation_core::controller::{Overlay, OverlayState, PasteOutcome, Paster, Recorder};

pub struct StubRecorder;

impl Recorder for StubRecorder {
    fn start(&mut self) -> Result<(), String> {
        log::warn!("recorder not implemented yet");
        Err("not implemented yet".into())
    }
    fn stop(&mut self) -> Result<Vec<f32>, String> {
        Err("not implemented yet".into())
    }
    fn cancel(&mut self) {}
}

pub struct StubPaster;

impl Paster for StubPaster {
    fn focused_app(&self) -> Option<String> {
        None
    }
    fn paste(&mut self, text: &str) -> PasteOutcome {
        // Never log the transcript itself.
        log::info!("paste stub ({} chars)", text.chars().count());
        PasteOutcome::CopiedOnly
    }
}

pub struct StubOverlay;

impl Overlay for StubOverlay {
    fn set(&self, s: OverlayState) {
        log::info!("overlay: {s:?}");
    }
}
