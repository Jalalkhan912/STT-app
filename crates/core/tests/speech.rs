use dictation_core::capture::rms;
use dictation_core::engine::Engine;
use dictation_core::speech::{Speech, SpeechError, SpeechPipeline};
use dictation_core::vad::SpeechDetector;
use std::sync::{Arc, Mutex};

struct FakeDetector {
    frame: usize,
}
impl SpeechDetector for FakeDetector {
    fn frame_size(&self) -> usize {
        self.frame
    }
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, SpeechError> {
        Ok(rms(frame) > 0.01)
    }
    fn reset(&mut self) {}
}

struct FakeEngine {
    lens: Arc<Mutex<Vec<usize>>>,
    blank: bool,
}
impl Engine for FakeEngine {
    fn id(&self) -> &str {
        "fake"
    }
    fn transcribe(&mut self, s: &[f32]) -> Result<String, SpeechError> {
        let mut l = self.lens.lock().unwrap();
        l.push(s.len());
        Ok(if self.blank {
            "  ".into()
        } else {
            format!("chunk{}", l.len())
        })
    }
}

fn build(blank: bool) -> (SpeechPipeline, Arc<Mutex<Vec<usize>>>) {
    let lens = Arc::new(Mutex::new(Vec::new()));
    let e = FakeEngine {
        lens: lens.clone(),
        blank,
    };
    (
        SpeechPipeline::new(Box::new(e), Box::new(FakeDetector { frame: 480 })),
        lens,
    )
}
fn pipeline() -> SpeechPipeline {
    build(false).0
}
fn pipeline_with_log() -> (SpeechPipeline, Arc<Mutex<Vec<usize>>>) {
    build(false)
}

#[test]
fn silence_is_no_speech() {
    assert_eq!(pipeline().process(&vec![0.0; 32_000]).unwrap(), None);
}

#[test]
fn short_speech_is_single_chunk() {
    let (mut p, lens) = pipeline_with_log();
    assert_eq!(
        p.process(&vec![0.2; 48_000]).unwrap(),
        Some("chunk1".into())
    );
    assert_eq!(*lens.lock().unwrap(), vec![48_000]);
}

#[test]
fn under_8_speech_frames_is_no_speech() {
    let mut s = vec![0.0; 32_000];
    for x in &mut s[..480 * 7] {
        *x = 0.2
    }
    assert_eq!(pipeline().process(&s).unwrap(), None);
}

#[test]
fn long_audio_chunks_at_silence_under_30s() {
    let total = 150 * 16_000;
    let mut s = vec![0.2; total];
    for sec in [25, 52, 80, 110, 135] {
        for x in &mut s[sec * 16_000..(sec + 1) * 16_000] {
            *x = 0.0;
        }
    }
    let (mut p, lens) = pipeline_with_log();
    assert!(p.process(&s).unwrap().is_some());
    let l = lens.lock().unwrap();
    assert!(l.iter().all(|&n| n <= 480_000), "{l:?}");
    assert_eq!(l.iter().sum::<usize>(), total);
    assert!((400_000..416_000).contains(&l[0]), "{l:?}");
}

#[test]
fn no_silence_forces_hard_cuts() {
    let (mut p, lens) = pipeline_with_log();
    p.process(&vec![0.2; 150 * 16_000]).unwrap();
    assert_eq!(*lens.lock().unwrap(), vec![480_000; 5]);
}

#[test]
fn empty_engine_text_is_no_speech() {
    let (mut p, _) = build(true);
    assert_eq!(p.process(&vec![0.2; 48_000]).unwrap(), None);
}
