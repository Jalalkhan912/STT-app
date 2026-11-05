use crate::capture::SAMPLE_RATE;
use std::io;
use std::path::Path;

fn to_io(e: hound::Error) -> io::Error {
    match e {
        hound::Error::IoError(e) => e,
        other => io::Error::new(io::ErrorKind::InvalidData, other),
    }
}

/// Writes mono 16-bit PCM at `SAMPLE_RATE`, clamping samples to [-1, 1].
pub fn write_wav(path: &Path, s: &[f32]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(to_io)?;
    for &x in s {
        let v = (x.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        w.write_sample(v).map_err(to_io)?;
    }
    w.finalize().map_err(to_io)
}

/// Reads a 16-bit PCM WAV into f32 samples in [-1, 1].
pub fn read_wav(path: &Path) -> io::Result<Vec<f32>> {
    let mut r = hound::WavReader::open(path).map_err(to_io)?;
    let spec = r.spec();
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected 16-bit PCM WAV",
        ));
    }
    r.samples::<i16>()
        .map(|v| v.map(|v| f32::from(v) / f32::from(i16::MAX)).map_err(to_io))
        .collect()
}
