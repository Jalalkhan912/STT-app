use std::collections::VecDeque;

pub const SAMPLE_RATE: u32 = 16_000;

const SAMPLES_PER_MS: usize = (SAMPLE_RATE / 1000) as usize;

#[derive(Debug, PartialEq)]
pub enum PushResult {
    Ok,
    MaxReached,
}

/// Mono 16 kHz capture buffer: keeps a rolling pre-roll while idle and
/// accumulates samples (pre-roll included) while recording.
pub struct CaptureBuffer {
    preroll_cap: usize,
    max_recorded: usize,
    preroll: VecDeque<f32>,
    recording: Vec<f32>,
    preroll_len: usize,
    active: bool,
}

impl CaptureBuffer {
    pub fn new(preroll_ms: u32, max_ms: u32) -> Self {
        let preroll_cap = preroll_ms as usize * SAMPLES_PER_MS;
        Self {
            preroll_cap,
            max_recorded: max_ms as usize * SAMPLES_PER_MS,
            preroll: VecDeque::with_capacity(preroll_cap),
            recording: Vec::new(),
            preroll_len: 0,
            active: false,
        }
    }

    pub fn push(&mut self, s: &[f32]) -> PushResult {
        if self.active {
            let recorded = self.recording.len() - self.preroll_len;
            let room = self.max_recorded.saturating_sub(recorded);
            let n = s.len().min(room);
            self.recording.extend_from_slice(&s[..n]);
            if recorded + n >= self.max_recorded {
                PushResult::MaxReached
            } else {
                PushResult::Ok
            }
        } else {
            // Only the last `preroll_cap` samples can survive.
            let s = &s[s.len().saturating_sub(self.preroll_cap)..];
            let overflow = (self.preroll.len() + s.len()).saturating_sub(self.preroll_cap);
            self.preroll.drain(..overflow);
            self.preroll.extend(s);
            PushResult::Ok
        }
    }

    pub fn start(&mut self) {
        self.recording.clear();
        self.recording.extend(self.preroll.drain(..));
        self.preroll_len = self.recording.len();
        self.active = true;
    }

    pub fn take(&mut self) -> Vec<f32> {
        self.active = false;
        self.preroll_len = 0;
        self.preroll.clear();
        std::mem::take(&mut self.recording)
    }

    pub fn discard(&mut self) {
        self.take();
    }

    pub fn is_recording(&self) -> bool {
        self.active
    }
}

pub fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}
