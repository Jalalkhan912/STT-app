//! Dictation controller: ties capture, speech, cleanup, paste and history together.
use crate::capture::SAMPLE_RATE;
use crate::cleanup::clean;
use crate::events::Event;
use crate::history::{History, NewDictation, Status};
use crate::settings::{CleanupConfig, Retention};
use crate::speech::Speech;
use serde::Serialize;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, RwLock, TryLockError};

/// Releases shorter than this are treated as accidental taps.
pub const MIN_HOLD_MS: u64 = 300;

pub trait Recorder: Send {
    fn start(&mut self) -> Result<(), String>;
    /// Returns the audio including pre-roll and the tail.
    fn stop(&mut self) -> Result<Vec<f32>, String>;
    fn cancel(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PasteOutcome {
    Pasted,
    CopiedOnly,
}

pub trait Paster: Send {
    fn focused_app(&self) -> Option<String>;
    fn paste(&mut self, text: &str) -> PasteOutcome;
    /// Puts the text on the clipboard without sending a paste keystroke.
    fn copy_only(&mut self, text: &str);
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum OverlayState {
    Hidden,
    Recording,
    Processing,
    Done,
    NoSpeech,
    Copied,
    MicError(String),
    NoModel,
    /// A press arrived while the model was still loading.
    Loading,
    Error(String),
}

pub trait Overlay: Send {
    fn set(&self, s: OverlayState);
}

pub type SharedSpeech = Arc<Mutex<Option<Box<dyn Speech>>>>;

pub struct ControllerConfig {
    pub cleanup: CleanupConfig,
    pub retention: Retention,
}

enum State {
    Idle,
    Recording {
        started_ms: u64,
        app: Option<String>,
    },
}

pub struct Controller {
    recorder: Box<dyn Recorder>,
    speech: SharedSpeech,
    paster: Box<dyn Paster>,
    overlay: Box<dyn Overlay>,
    history: Arc<History>,
    config: Arc<RwLock<ControllerConfig>>,
    now_ms: Box<dyn Fn() -> i64 + Send>,
    state: State,
    on_saved: Option<Box<dyn Fn() + Send>>,
}

impl Controller {
    pub fn new(
        recorder: Box<dyn Recorder>,
        speech: SharedSpeech,
        paster: Box<dyn Paster>,
        overlay: Box<dyn Overlay>,
        history: Arc<History>,
        config: Arc<RwLock<ControllerConfig>>,
        now_ms: Box<dyn Fn() -> i64 + Send>,
    ) -> Self {
        Controller {
            recorder,
            speech,
            paster,
            overlay,
            history,
            config,
            now_ms,
            state: State::Idle,
            on_saved: None,
        }
    }

    /// Called after a dictation row has been written to history.
    pub fn set_on_saved(&mut self, f: Box<dyn Fn() + Send>) {
        self.on_saved = Some(f);
    }

    pub fn handle(&mut self, ev: Event) {
        self.handle_inner(ev);
    }

    /// Returns true when the event ran the transcription pipeline.
    fn handle_inner(&mut self, ev: Event) -> bool {
        match (&self.state, ev) {
            (State::Idle, Event::Pressed { at_ms }) => self.on_press(at_ms),
            (State::Recording { .. }, Event::Cancel) => self.cancel(),
            (State::Recording { started_ms, .. }, Event::Released { at_ms }) => {
                if at_ms.saturating_sub(*started_ms) < MIN_HOLD_MS {
                    self.cancel();
                } else {
                    return self.finish();
                }
            }
            (State::Recording { .. }, Event::MaxDuration) => return self.finish(),
            _ => {}
        }
        false
    }

    fn on_press(&mut self, at_ms: u64) {
        // try_lock: a busy lock means inference or a retranscribe holds the
        // model, so it is loaded. Never wait for it here.
        match self.speech.try_lock() {
            Ok(guard) if guard.is_none() => {
                self.overlay.set(OverlayState::NoModel);
                return;
            }
            Err(TryLockError::Poisoned(_)) => {
                self.overlay
                    .set(OverlayState::Error("speech engine unavailable".into()));
                return;
            }
            Ok(_) | Err(TryLockError::WouldBlock) => {}
        }
        // Feedback first: opening the audio stream can take a while.
        self.overlay.set(OverlayState::Recording);
        let app = self.paster.focused_app();
        if let Err(e) = self.recorder.start() {
            self.overlay.set(OverlayState::MicError(e));
            return;
        }
        self.state = State::Recording {
            started_ms: at_ms,
            app,
        };
    }

    fn cancel(&mut self) {
        self.recorder.cancel();
        self.overlay.set(OverlayState::Hidden);
        self.state = State::Idle;
    }

    fn finish(&mut self) -> bool {
        let State::Recording { app, .. } = std::mem::replace(&mut self.state, State::Idle) else {
            return false;
        };
        self.overlay.set(OverlayState::Processing);
        let samples = match self.recorder.stop() {
            Ok(s) => s,
            Err(e) => {
                self.overlay.set(OverlayState::MicError(e));
                return true;
            }
        };
        let duration_ms = (samples.len() as u64 * 1000 / SAMPLE_RATE as u64) as i64;
        let (cleanup, retention) = {
            let c = self.config.read().unwrap_or_else(|e| e.into_inner());
            (c.cleanup.clone(), c.retention.clone())
        };
        let keep_audio = retention != Retention::Never;

        let (model, result) = match self.speech.lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(sp) => (sp.model_id(), sp.process(&samples)),
                None => {
                    self.overlay.set(OverlayState::NoModel);
                    return true;
                }
            },
            Err(_) => {
                self.overlay
                    .set(OverlayState::Error("speech engine unavailable".into()));
                return true;
            }
        };

        let mut row = NewDictation {
            created_at_ms: (self.now_ms)(),
            duration_ms,
            text: String::new(),
            raw_text: String::new(),
            model,
            target_app: app.clone(),
            status: Status::NoSpeech,
        };
        let mut audio: Option<&[f32]> = None;
        match result {
            Err(e) => {
                self.overlay.set(OverlayState::Error(e.to_string()));
                row.status = Status::Error;
                audio = keep_audio.then_some(samples.as_slice());
            }
            Ok(raw) => {
                let raw = raw.unwrap_or_default();
                let text = clean(&raw, &cleanup);
                if text.is_empty() {
                    self.overlay.set(OverlayState::NoSpeech);
                } else {
                    row.raw_text = raw;
                    audio = keep_audio.then_some(samples.as_slice());
                    let focus_moved = matches!(
                        (&app, self.paster.focused_app()),
                        (Some(a), Some(b)) if *a != b
                    );
                    let outcome = if focus_moved {
                        self.paster.copy_only(&text);
                        PasteOutcome::CopiedOnly
                    } else {
                        self.paster.paste(&text)
                    };
                    match outcome {
                        PasteOutcome::Pasted => {
                            self.overlay.set(OverlayState::Done);
                            row.status = Status::Pasted;
                        }
                        PasteOutcome::CopiedOnly => {
                            self.overlay.set(OverlayState::Copied);
                            row.status = Status::CopiedOnly;
                        }
                    }
                    row.text = text;
                }
            }
        }
        match self.history.add(row, audio) {
            Ok(_) => {
                if let Some(f) = &self.on_saved {
                    f();
                }
            }
            Err(e) => eprintln!("failed to save dictation: {e}"),
        }
        true
    }
}

pub fn run(mut ctrl: Controller, rx: Receiver<Event>) {
    while let Ok(ev) = rx.recv() {
        if ctrl.handle_inner(ev) {
            while let Ok(queued) = rx.try_recv() {
                if !matches!(queued, Event::Pressed { .. } | Event::Released { .. }) {
                    ctrl.handle_inner(queued);
                }
            }
        }
    }
}
