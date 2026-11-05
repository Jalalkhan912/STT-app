#![cfg(feature = "parakeet")]
//! Engine accuracy/speed benchmark. Needs the Parakeet model:
//! `DICTATION_MODELS=~/models cargo test -p dictation-core --features parakeet --test accuracy -- --ignored --nocapture`
use dictation_core::engine::{warm_up, Engine, ParakeetEngine};
use std::path::PathBuf;
use std::time::Instant;

fn normalize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(String::from)
        .collect()
}

fn wer(reference: &str, hyp: &str) -> f64 {
    let (r, h) = (normalize(reference), normalize(hyp));
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur.push(sub.min(prev[j] + 1).min(cur[j - 1] + 1));
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len().max(1) as f64
}

#[test]
fn wer_helper_sanity() {
    assert_eq!(wer("Hello, world", "hello world"), 0.0);
    assert_eq!(wer("a b c d", "a x c"), 0.5);
}

#[test]
#[ignore]
fn parakeet_accuracy() {
    let models = std::env::var("DICTATION_MODELS").expect("set DICTATION_MODELS");
    let dir = PathBuf::from(models).join("parakeet-tdt-0.6b-v2-int8");
    let mut engine = ParakeetEngine::load(&dir).unwrap();
    warm_up(&mut engine).unwrap();

    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut wers = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(&fixtures)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    paths.sort();
    for wav in paths {
        let txt = wav.with_extension("txt");
        if !txt.exists() {
            continue;
        }
        let mut reader = hound::WavReader::open(&wav).unwrap();
        let rate = reader.spec().sample_rate as f32;
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        let start = Instant::now();
        let text = engine.transcribe(&samples).unwrap();
        let rtf = start.elapsed().as_secs_f64() / (samples.len() as f64 / rate as f64);
        let w = wer(&std::fs::read_to_string(&txt).unwrap(), &text);
        println!(
            "WER {}: {:.2}%  RTF: {:.3}",
            wav.file_name().unwrap().to_string_lossy(),
            w * 100.0,
            rtf
        );
        wers.push(w);
    }
    assert!(!wers.is_empty(), "no fixtures found");
    let mean = wers.iter().sum::<f64>() / wers.len() as f64;
    assert!(mean < 0.08, "mean WER {mean}");
}
