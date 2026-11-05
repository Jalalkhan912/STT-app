//! Speech-to-text engines. Real adapters are behind cargo features.
use crate::speech::SpeechError;

pub trait Engine: Send {
    fn id(&self) -> &str;
    fn transcribe(&mut self, s: &[f32]) -> Result<String, SpeechError>;
}

/// Run one second of silence through the engine to pay one-time init costs.
pub fn warm_up(e: &mut dyn Engine) -> Result<(), SpeechError> {
    e.transcribe(&vec![0.0; crate::capture::SAMPLE_RATE as usize])
        .map(|_| ())
}

#[cfg(feature = "parakeet")]
pub use parakeet::{ParakeetEngine, SileroDetector};

#[cfg(feature = "parakeet")]
mod parakeet {
    use super::Engine;
    use crate::speech::SpeechError;
    use crate::vad::SpeechDetector;
    use std::path::Path;
    use transcribe_rs::onnx::parakeet::ParakeetModel;
    use transcribe_rs::onnx::Quantization;
    use transcribe_rs::vad::{SileroVad, Vad};
    use transcribe_rs::{SpeechModel, TranscribeOptions};

    pub struct ParakeetEngine(ParakeetModel);

    impl ParakeetEngine {
        pub fn load(dir: &Path) -> Result<Self, SpeechError> {
            ParakeetModel::load(dir, &Quantization::Int8)
                .map(Self)
                .map_err(|e| SpeechError::Engine(e.to_string()))
        }
    }

    impl Engine for ParakeetEngine {
        fn id(&self) -> &str {
            "parakeet-tdt-0.6b-v2-int8"
        }
        fn transcribe(&mut self, s: &[f32]) -> Result<String, SpeechError> {
            self.0
                .transcribe(s, &TranscribeOptions::default())
                .map(|r| r.text)
                .map_err(|e| SpeechError::Engine(e.to_string()))
        }
    }

    pub struct SileroDetector(SileroVad);

    impl SileroDetector {
        pub fn load(path: &Path) -> Result<Self, SpeechError> {
            SileroVad::new(path, 0.3)
                .map(Self)
                .map_err(|e| SpeechError::Vad(e.to_string()))
        }
    }

    impl SpeechDetector for SileroDetector {
        fn frame_size(&self) -> usize {
            Vad::frame_size(&self.0)
        }
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool, SpeechError> {
            Vad::is_speech(&mut self.0, frame).map_err(|e| SpeechError::Vad(e.to_string()))
        }
        fn reset(&mut self) {
            Vad::reset(&mut self.0)
        }
    }
}

#[cfg(feature = "whisper")]
pub use whisper::WhisperEngine;

#[cfg(feature = "whisper")]
mod whisper {
    use super::Engine;
    use crate::speech::SpeechError;
    use std::path::Path;
    use transcribe_rs::whisper_cpp::{WhisperEngine as Inner, WhisperInferenceParams};

    pub struct WhisperEngine(Inner);

    impl WhisperEngine {
        pub fn load(file: &Path) -> Result<Self, SpeechError> {
            Inner::load(file)
                .map(Self)
                .map_err(|e| SpeechError::Engine(e.to_string()))
        }
    }

    impl Engine for WhisperEngine {
        fn id(&self) -> &str {
            "whisper"
        }
        fn transcribe(&mut self, s: &[f32]) -> Result<String, SpeechError> {
            let params = WhisperInferenceParams {
                language: Some("en".into()),
                ..Default::default()
            };
            self.0
                .transcribe_with(s, &params)
                .map(|r| r.text)
                .map_err(|e| SpeechError::Engine(e.to_string()))
        }
    }
}
