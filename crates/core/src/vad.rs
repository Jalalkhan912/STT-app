//! Voice-activity detection helpers and silence-aware chunk planning.
use crate::speech::SpeechError;
use std::ops::Range;

/// Minimum number of speech frames for audio to count as speech.
const MIN_SPEECH_FRAMES: usize = 8;

pub trait SpeechDetector: Send {
    fn frame_size(&self) -> usize;
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool, SpeechError>;
    fn reset(&mut self);
}

/// Classify every frame of `s`; the last partial frame is zero-padded.
pub fn speech_frames(det: &mut dyn SpeechDetector, s: &[f32]) -> Result<Vec<bool>, SpeechError> {
    let n = det.frame_size();
    let mut flags = Vec::with_capacity(s.len() / n + 1);
    for frame in s.chunks(n) {
        if frame.len() == n {
            flags.push(det.is_speech(frame)?);
        } else {
            let mut padded = frame.to_vec();
            padded.resize(n, 0.0);
            flags.push(det.is_speech(&padded)?);
        }
    }
    Ok(flags)
}

pub fn has_speech(flags: &[bool]) -> bool {
    flags.iter().filter(|&&f| f).count() >= MIN_SPEECH_FRAMES
}

/// Split `total` samples into chunks of at most `max_chunk`, cutting in the
/// middle of the longest silence in the second half of each window.
pub fn plan_chunks(
    flags: &[bool],
    frame: usize,
    total: usize,
    max_chunk: usize,
) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while total - cursor > max_chunk {
        let end = cursor + max_chunk;
        let first = (cursor + max_chunk / 2).div_ceil(frame);
        let last = (end / frame).min(flags.len());
        let mut best: Option<(usize, usize)> = None; // (start frame, len)
        let mut run_start = None;
        #[allow(clippy::needless_range_loop)]
        for i in first..=last {
            let silent = flags.get(i).is_some_and(|&f| !f) && i < last;
            match (silent, run_start) {
                (true, None) => run_start = Some(i),
                (false, Some(s)) => {
                    if best.is_none_or(|(_, l)| i - s > l) {
                        best = Some((s, i - s));
                    }
                    run_start = None;
                }
                _ => {}
            }
        }
        let cut = match best {
            Some((s, l)) => ((s * frame + (s + l) * frame) / 2).clamp(cursor + 1, end),
            None => end,
        };
        out.push(cursor..cut);
        cursor = cut;
    }
    out.push(cursor..total);
    out
}
