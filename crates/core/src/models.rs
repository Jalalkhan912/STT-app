//! Model catalog plus resumable, checksum-verified download and import.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

const PROGRESS_STEP: u64 = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub enum EngineKind {
    Parakeet,
    Whisper,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub kind: EngineKind,
    pub files: Vec<ModelFile>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("checksum mismatch for {file}")]
    ChecksumMismatch { file: String },
    #[error("cancelled")]
    Cancelled,
    #[error("unknown model file: {0}")]
    UnknownFile(String),
}

fn file(repo: &str, name: &str, size: u64, sha256: &str) -> ModelFile {
    ModelFile {
        name: name.into(),
        url: format!("https://huggingface.co/{repo}/resolve/main/{name}"),
        sha256: sha256.into(),
        size,
    }
}

pub fn catalog() -> Vec<ModelInfo> {
    let p = "istupakov/parakeet-tdt-0.6b-v2-onnx";
    vec![
        ModelInfo {
            id: "parakeet-tdt-0.6b-v2-int8".into(),
            label: "Parakeet TDT 0.6B v2 (English, recommended)".into(),
            kind: EngineKind::Parakeet,
            files: vec![
                file(
                    p,
                    "encoder-model.int8.onnx",
                    652184014,
                    "3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155",
                ),
                file(
                    p,
                    "decoder_joint-model.int8.onnx",
                    8998286,
                    "a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3",
                ),
                file(
                    p,
                    "nemo128.onnx",
                    139764,
                    "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
                ),
                file(
                    p,
                    "vocab.txt",
                    9384,
                    "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d",
                ),
            ],
        },
        ModelInfo {
            id: "whisper-large-v3-turbo-q5".into(),
            label: "Whisper large-v3 turbo (GPU)".into(),
            kind: EngineKind::Whisper,
            files: vec![file(
                "ggerganov/whisper.cpp",
                "ggml-large-v3-turbo-q5_0.bin",
                574041195,
                "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
            )],
        },
    ]
}

pub fn find(id: &str) -> Option<ModelInfo> {
    catalog().into_iter().find(|m| m.id == id)
}

pub fn model_dir(models_root: &Path, id: &str) -> PathBuf {
    models_root.join(id)
}

pub fn is_installed(models_root: &Path, m: &ModelInfo) -> bool {
    let dir = model_dir(models_root, &m.id);
    m.files
        .iter()
        .all(|f| fs::metadata(dir.join(&f.name)).is_ok_and(|md| md.len() == f.size))
}

fn part_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.part"))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Verify the `.part` file against the expected hash; rename on success, delete on failure.
fn verify_and_finalize(dir: &Path, f: &ModelFile) -> Result<(), ModelError> {
    let part = part_path(dir, &f.name);
    if sha256_file(&part)? != f.sha256.to_lowercase() {
        let _ = fs::remove_file(&part);
        return Err(ModelError::ChecksumMismatch {
            file: f.name.clone(),
        });
    }
    fs::rename(&part, dir.join(&f.name))?;
    Ok(())
}

fn client() -> Result<reqwest::blocking::Client, ModelError> {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // reqwest is built without a bundled provider; use ring (ignore if already set).
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| ModelError::Http(e.to_string()))
}

pub fn install(
    models_root: &Path,
    m: &ModelInfo,
    progress: &dyn Fn(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), ModelError> {
    let dir = model_dir(models_root, &m.id);
    fs::create_dir_all(&dir)?;
    let total: u64 = m.files.iter().map(|f| f.size).sum();
    let client = client()?;
    let mut done_files = 0u64;

    for f in &m.files {
        if cancel.load(Ordering::Relaxed) {
            return Err(ModelError::Cancelled);
        }
        let final_path = dir.join(&f.name);
        if fs::metadata(&final_path).is_ok_and(|md| md.len() == f.size) {
            done_files += f.size;
            continue;
        }
        let part = part_path(&dir, &f.name);
        let mut have = fs::metadata(&part).map(|md| md.len()).unwrap_or(0);
        if have > f.size {
            fs::remove_file(&part)?;
            have = 0;
        }
        if have < f.size {
            let mut req = client.get(&f.url);
            if have > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
            }
            let mut resp = req.send().map_err(|e| ModelError::Http(e.to_string()))?;
            let mut out = match resp.status().as_u16() {
                206 if have > 0 => OpenOptions::new().append(true).open(&part)?,
                200 => {
                    have = 0;
                    File::create(&part)?
                }
                s => {
                    return Err(ModelError::Http(format!(
                        "unexpected status {s} for {}",
                        f.url
                    )))
                }
            };
            let mut buf = vec![0u8; 64 * 1024];
            let mut got = have;
            let mut last_report;
            progress(done_files + got, total);
            last_report = done_files + got;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    out.flush()?;
                    return Err(ModelError::Cancelled);
                }
                let n = resp.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                got += n as u64;
                if done_files + got - last_report >= PROGRESS_STEP {
                    last_report = done_files + got;
                    progress(last_report, total);
                }
            }
            out.flush()?;
        }
        verify_and_finalize(&dir, f)?;
        done_files += f.size;
    }
    progress(total, total);
    Ok(())
}

pub fn import_file(
    models_root: &Path,
    m: &ModelInfo,
    file_name: &str,
    src: &Path,
) -> Result<(), ModelError> {
    let f = m
        .files
        .iter()
        .find(|f| f.name == file_name)
        .ok_or_else(|| ModelError::UnknownFile(file_name.to_string()))?;
    let dir = model_dir(models_root, &m.id);
    fs::create_dir_all(&dir)?;
    fs::copy(src, part_path(&dir, &f.name))?;
    verify_and_finalize(&dir, f)
}
