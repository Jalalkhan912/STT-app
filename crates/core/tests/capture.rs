use dictation_core::{capture::*, wav::*};

#[test]
fn keeps_only_preroll_when_idle() {
    let mut b = CaptureBuffer::new(200, 60_000);
    b.push(&vec![0.1; 10_000]);
    b.start();
    b.push(&vec![0.5; 100]);
    let s = b.take();
    assert_eq!(s.len(), 3_300);
    assert!(s[..3_200].iter().all(|&x| x == 0.1) && s[3_200..].iter().all(|&x| x == 0.5));
    assert!(!b.is_recording());
}
#[test]
fn reports_max_and_caps_length() {
    let mut b = CaptureBuffer::new(0, 1_000);
    b.start();
    assert_eq!(b.push(&vec![0.0; 15_999]), PushResult::Ok);
    assert_eq!(b.push(&[0.0; 10]), PushResult::MaxReached);
    assert_eq!(b.take().len(), 16_000);
}
#[test]
fn discard_clears() {
    let mut b = CaptureBuffer::new(200, 1_000);
    b.start();
    b.push(&[0.3; 50]);
    b.discard();
    b.start();
    assert!(b.take().is_empty());
}
#[test]
fn rms_value() {
    assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
    assert_eq!(rms(&[]), 0.0);
}
#[test]
fn wav_roundtrip_and_clamp() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a/b/x.wav");
    write_wav(&p, &[0.0, 0.5, -0.5, 2.0]).unwrap();
    let spec = hound::WavReader::open(&p).unwrap().spec();
    assert_eq!(
        (spec.sample_rate, spec.channels, spec.bits_per_sample),
        (16_000, 1, 16)
    );
    let r = read_wav(&p).unwrap();
    assert_eq!(r.len(), 4);
    assert!((r[1] - 0.5).abs() < 1e-3 && (r[3] - 1.0).abs() < 1e-3);
}
