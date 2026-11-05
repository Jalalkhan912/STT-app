//! Speech pipeline: VAD gate, chunking and transcription.
use crate::engine::Engine;
use crate::vad::{has_speech, plan_chunks, speech_frames, SpeechDetector};

/// Audio up to this length (2 min) is transcribed as a single chunk.
const SINGLE_CHUNK_MAX: usize = 1_920_000;
/// Maximum chunk length (30 s) when splitting long audio.
const MAX_CHUNK: usize = 480_000;

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("engine error: {0}")]
    Engine(String),
    #[error("vad error: {0}")]
    Vad(String),
}

pub trait Speech: Send {
    fn model_id(&self) -> String;
    /// `None` means no speech was found.
    fn process(&mut self, s: &[f32]) -> Result<Option<String>, SpeechError>;
}

pub struct SpeechPipeline {
    engine: Box<dyn Engine>,
    detector: Box<dyn SpeechDetector>,
}

impl SpeechPipeline {
    pub fn new(engine: Box<dyn Engine>, detector: Box<dyn SpeechDetector>) -> Self {
        Self { engine, detector }
    }

    fn run(&mut self, s: &[f32], flags: &[bool]) -> Result<Option<String>, SpeechError> {
        let ranges = if s.len() <= SINGLE_CHUNK_MAX {
            std::iter::once(0..s.len()).collect()
        } else {
            plan_chunks(flags, self.detector.frame_size(), s.len(), MAX_CHUNK)
        };
        let mut texts = Vec::new();
        for r in ranges {
            let t = self.engine.transcribe(&s[r])?;
            let t = t.trim();
            if !t.is_empty() {
                texts.push(t.to_string());
            }
        }
        Ok((!texts.is_empty()).then(|| texts.join(" ")))
    }
}

impl Speech for SpeechPipeline {
    fn model_id(&self) -> String {
        self.engine.id().to_string()
    }

    fn process(&mut self, s: &[f32]) -> Result<Option<String>, SpeechError> {
        let flags = match speech_frames(self.detector.as_mut(), s) {
            Ok(f) => f,
            Err(e) => {
                self.detector.reset();
                return Err(e);
            }
        };
        if !has_speech(&flags) {
            self.detector.reset();
            return Ok(None);
        }
        let out = self.run(s, &flags);
        self.detector.reset();
        out
    }
}
