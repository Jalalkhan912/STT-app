use dictation_core::controller::*;
use dictation_core::events::Event;
use dictation_core::history::{History, Status};
use dictation_core::settings::{CleanupConfig, Retention};
use dictation_core::speech::{Speech, SpeechError};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Default)]
struct RecState {
    fail_start: bool,
    fail_stop: bool,
    calls: Vec<&'static str>,
}

struct FakeRecorder(Arc<Mutex<RecState>>);
impl Recorder for FakeRecorder {
    fn start(&mut self) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("start");
        if s.fail_start {
            Err("no mic".into())
        } else {
            Ok(())
        }
    }
    fn stop(&mut self) -> Result<Vec<f32>, String> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("stop");
        if s.fail_stop {
            Err("stream lost".into())
        } else {
            Ok(vec![0.1; 16_000])
        }
    }
    fn cancel(&mut self) {
        self.0.lock().unwrap().calls.push("cancel");
    }
}

struct FakeSpeech(Result<Option<String>, String>);
impl Speech for FakeSpeech {
    fn model_id(&self) -> String {
        "fake-model".into()
    }
    fn process(&mut self, _s: &[f32]) -> Result<Option<String>, SpeechError> {
        self.0.clone().map_err(SpeechError::Engine)
    }
}

struct FakePaster {
    outcome: PasteOutcome,
    log: Arc<Mutex<Vec<String>>>,
}
impl Paster for FakePaster {
    fn focused_app(&self) -> Option<String> {
        Some("TestApp".into())
    }
    fn paste(&mut self, text: &str) -> PasteOutcome {
        self.log.lock().unwrap().push(text.to_string());
        self.outcome
    }
}

struct FakeOverlay(Arc<Mutex<Vec<OverlayState>>>);
impl Overlay for FakeOverlay {
    fn set(&self, s: OverlayState) {
        self.0.lock().unwrap().push(s);
    }
}

struct Rig {
    ctrl: Controller,
    rec: Arc<Mutex<RecState>>,
    pasted: Arc<Mutex<Vec<String>>>,
    overlay: Arc<Mutex<Vec<OverlayState>>>,
    history: Arc<History>,
    config: Arc<RwLock<ControllerConfig>>,
    _dir: tempfile::TempDir,
}

fn rig_with(speech: Option<Result<Option<String>, String>>, outcome: PasteOutcome) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let history = Arc::new(History::open(dir.path()).unwrap());
    let rec = Arc::new(Mutex::new(RecState::default()));
    let pasted = Arc::new(Mutex::new(Vec::new()));
    let overlay = Arc::new(Mutex::new(Vec::new()));
    let config = Arc::new(RwLock::new(ControllerConfig {
        cleanup: CleanupConfig::default(),
        retention: Retention::Forever,
    }));
    let speech: SharedSpeech = Arc::new(Mutex::new(
        speech.map(|r| Box::new(FakeSpeech(r)) as Box<dyn Speech>),
    ));
    let ctrl = Controller::new(
        Box::new(FakeRecorder(rec.clone())),
        speech,
        Box::new(FakePaster {
            outcome,
            log: pasted.clone(),
        }),
        Box::new(FakeOverlay(overlay.clone())),
        history.clone(),
        config.clone(),
        Box::new(|| 1_700_000_000_000),
    );
    Rig {
        ctrl,
        rec,
        pasted,
        overlay,
        history,
        config,
        _dir: dir,
    }
}

fn rig(reply: Result<Option<String>, String>) -> Rig {
    rig_with(Some(reply), PasteOutcome::Pasted)
}

fn press_release(r: &mut Rig, a: u64, b: u64) {
    r.ctrl.handle(Event::Pressed { at_ms: a });
    r.ctrl.handle(Event::Released { at_ms: b });
}

#[test]
fn happy_path_pastes_cleaned_text_and_saves() {
    let mut r = rig(Ok(Some("um, hello world.".into())));
    press_release(&mut r, 0, 1000);
    assert_eq!(*r.pasted.lock().unwrap(), vec!["Hello world. "]);
    assert_eq!(
        *r.overlay.lock().unwrap(),
        vec![
            OverlayState::Recording,
            OverlayState::Processing,
            OverlayState::Done
        ]
    );
    let rows = r.history.list(None, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, Status::Pasted);
    assert_eq!(rows[0].text, "Hello world. ");
    assert_eq!(rows[0].raw_text, "um, hello world.");
    assert!(rows[0].audio_path.is_some());
    assert_eq!(rows[0].target_app.as_deref(), Some("TestApp"));
    assert_eq!(rows[0].duration_ms, 1000);
    assert_eq!(rows[0].model, "fake-model");
    assert_eq!(rows[0].created_at_ms, 1_700_000_000_000);
}

fn assert_cancelled(r: &Rig) {
    assert!(r.rec.lock().unwrap().calls.contains(&"cancel"));
    assert_eq!(
        *r.overlay.lock().unwrap(),
        vec![OverlayState::Recording, OverlayState::Hidden]
    );
    assert!(r.history.list(None, 10).unwrap().is_empty());
    assert!(r.pasted.lock().unwrap().is_empty());
}

#[test]
fn short_tap_cancels() {
    let mut r = rig(Ok(Some("hi".into())));
    press_release(&mut r, 0, 200);
    assert_cancelled(&r);
}

#[test]
fn escape_cancels() {
    let mut r = rig(Ok(Some("hi".into())));
    r.ctrl.handle(Event::Pressed { at_ms: 0 });
    r.ctrl.handle(Event::Cancel);
    assert_cancelled(&r);
}

#[test]
fn no_model_shows_message() {
    let mut r = rig_with(None, PasteOutcome::Pasted);
    r.ctrl.handle(Event::Pressed { at_ms: 0 });
    assert_eq!(*r.overlay.lock().unwrap(), vec![OverlayState::NoModel]);
    assert!(!r.rec.lock().unwrap().calls.contains(&"start"));
}

#[test]
fn mic_start_failure_shows_mic_error() {
    let mut r = rig(Ok(Some("hi".into())));
    r.rec.lock().unwrap().fail_start = true;
    r.ctrl.handle(Event::Pressed { at_ms: 0 });
    assert!(matches!(
        r.overlay.lock().unwrap().as_slice(),
        [OverlayState::MicError(_)]
    ));
    r.ctrl.handle(Event::Released { at_ms: 1000 });
    assert!(!r.rec.lock().unwrap().calls.contains(&"stop"));
}

#[test]
fn no_speech_saves_row_without_audio() {
    let mut r = rig(Ok(None));
    press_release(&mut r, 0, 1000);
    assert_eq!(
        r.overlay.lock().unwrap().last(),
        Some(&OverlayState::NoSpeech)
    );
    let rows = r.history.list(None, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, Status::NoSpeech);
    assert_eq!(rows[0].audio_path, None);
    assert!(r.pasted.lock().unwrap().is_empty());
}

#[test]
fn only_fillers_is_no_speech() {
    let mut r = rig(Ok(Some("Um, uh.".into())));
    press_release(&mut r, 0, 1000);
    assert_eq!(
        r.overlay.lock().unwrap().last(),
        Some(&OverlayState::NoSpeech)
    );
    assert!(r.pasted.lock().unwrap().is_empty());
    let rows = r.history.list(None, 10).unwrap();
    assert_eq!(rows[0].status, Status::NoSpeech);
}

#[test]
fn engine_error_keeps_audio() {
    let mut r = rig(Err("boom".into()));
    press_release(&mut r, 0, 1000);
    assert!(matches!(
        r.overlay.lock().unwrap().last(),
        Some(OverlayState::Error(_))
    ));
    let rows = r.history.list(None, 10).unwrap();
    assert_eq!(rows[0].status, Status::Error);
    assert!(rows[0].audio_path.is_some());
    assert!(r.pasted.lock().unwrap().is_empty());
}

#[test]
fn copied_only_when_paste_fails() {
    let mut r = rig_with(Some(Ok(Some("hello".into()))), PasteOutcome::CopiedOnly);
    press_release(&mut r, 0, 1000);
    assert_eq!(
        r.overlay.lock().unwrap().last(),
        Some(&OverlayState::Copied)
    );
    assert_eq!(
        r.history.list(None, 10).unwrap()[0].status,
        Status::CopiedOnly
    );
}

#[test]
fn retention_never_skips_audio() {
    let mut r = rig(Ok(Some("hello".into())));
    r.config.write().unwrap().retention = Retention::Never;
    press_release(&mut r, 0, 1000);
    assert_eq!(r.history.list(None, 10).unwrap()[0].audio_path, None);
}

#[test]
fn recorder_failure_shows_mic_error_and_recovers() {
    let mut r = rig(Ok(Some("hello".into())));
    r.rec.lock().unwrap().fail_stop = true;
    press_release(&mut r, 0, 1000);
    assert!(matches!(
        r.overlay.lock().unwrap().last(),
        Some(OverlayState::MicError(_))
    ));
    assert!(r.history.list(None, 10).unwrap().is_empty());
    assert!(r.pasted.lock().unwrap().is_empty());

    r.rec.lock().unwrap().fail_stop = false;
    press_release(&mut r, 2000, 3000);
    assert_eq!(r.overlay.lock().unwrap().last(), Some(&OverlayState::Done));
    assert_eq!(r.history.list(None, 10).unwrap().len(), 1);
}

#[test]
fn max_duration_processes_like_release() {
    let mut r = rig(Ok(Some("hello".into())));
    r.ctrl.handle(Event::Pressed { at_ms: 0 });
    r.ctrl.handle(Event::MaxDuration);
    assert_eq!(r.overlay.lock().unwrap().last(), Some(&OverlayState::Done));
    assert_eq!(r.history.list(None, 10).unwrap().len(), 1);
}

#[test]
fn stray_events_are_ignored_while_idle() {
    let mut r = rig(Ok(Some("hello".into())));
    r.ctrl.handle(Event::Released { at_ms: 5 });
    r.ctrl.handle(Event::Cancel);
    r.ctrl.handle(Event::MaxDuration);
    assert!(r.overlay.lock().unwrap().is_empty());
    assert!(r.rec.lock().unwrap().calls.is_empty());
}

#[test]
fn run_drops_presses_queued_during_processing() {
    let r = rig(Ok(Some("hello".into())));
    let (tx, rx) = mpsc::channel();
    for ev in [
        Event::Pressed { at_ms: 0 },
        Event::Released { at_ms: 1000 },
        Event::Pressed { at_ms: 1100 },
        Event::Released { at_ms: 2000 },
    ] {
        tx.send(ev).unwrap();
    }
    drop(tx);
    run(r.ctrl, rx);
    assert_eq!(r.history.list(None, 10).unwrap().len(), 1);
    assert_eq!(r.pasted.lock().unwrap().len(), 1);
}
