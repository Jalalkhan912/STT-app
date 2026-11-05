use crate::paths;
use crate::state::{now_ms, AppState};
use dictation_core::models::{self, EngineKind, ModelInfo};
use dictation_core::speech::SpeechPipeline;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// Where the engine loads from: the model dir for Parakeet, the single
/// `.bin` file inside it for Whisper.
pub fn engine_path(models_root: &Path, m: &ModelInfo) -> PathBuf {
    let dir = models::model_dir(models_root, &m.id);
    match m.kind {
        EngineKind::Parakeet => dir,
        EngineKind::Whisper => dir.join(&m.files[0].name),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build_pipeline(
    models_root: &Path,
    m: &ModelInfo,
    vad_path: &Path,
) -> Result<SpeechPipeline, String> {
    use dictation_core::engine::{warm_up, Engine, ParakeetEngine, SileroDetector, WhisperEngine};
    let path = engine_path(models_root, m);
    let mut engine: Box<dyn Engine> = match m.kind {
        EngineKind::Parakeet => Box::new(ParakeetEngine::load(&path).map_err(|e| e.to_string())?),
        EngineKind::Whisper => {
            Box::new(WhisperEngine::load(&path, m.id.clone()).map_err(|e| e.to_string())?)
        }
    };
    warm_up(engine.as_mut()).map_err(|e| e.to_string())?;
    let detector = SileroDetector::load(vad_path).map_err(|e| e.to_string())?;
    Ok(SpeechPipeline::new(engine, Box::new(detector)))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn build_pipeline(
    _models_root: &Path,
    _m: &ModelInfo,
    _vad_path: &Path,
) -> Result<SpeechPipeline, String> {
    Err("speech engines are only available on macOS and Windows".into())
}

/// Builds and warms the pipeline for `settings.active_model` without holding
/// the speech lock, then locks only to swap it in.
///
/// With `force == false` it returns early if a model is already loaded
/// (checked after taking the load lock, so concurrent callers don't rebuild).
pub fn load_active(state: &AppState, vad_path: &Path, force: bool) -> Result<(), String> {
    let _serial = state.load_lock.lock().unwrap_or_else(|e| e.into_inner());
    if !force && state.speech.lock().map(|s| s.is_some()).unwrap_or(false) {
        return Ok(());
    }
    let id = state.settings_snapshot().active_model;
    let m = models::find(&id).ok_or_else(|| format!("unknown model {id}"))?;
    let root = paths::models_dir();
    if !models::is_installed(&root, &m) {
        let old = state
            .speech
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        drop(old);
        return Err(format!("model {id} is not installed"));
    }
    log::info!("loading model {id}");
    let pipeline = build_pipeline(&root, &m, vad_path).map_err(|e| {
        log::error!("loading model {id} failed: {e}");
        e
    })?;
    let old = state
        .speech
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Box::new(pipeline));
    drop(old);
    state.touch();
    log::info!("model {id} ready");
    Ok(())
}

/// Loads the active model on a background thread.
pub fn spawn_load(state: Arc<AppState>) {
    std::thread::spawn(move || {
        if let Err(e) = load_active(&state, &state.vad_path, true) {
            log::warn!("model load: {e}");
        }
    });
}

/// Synchronous load used by the hotkey handler when the model was unloaded
/// for idleness. Does nothing if a model is already loaded.
pub fn ensure_loaded(state: &AppState) {
    if let Err(e) = load_active(state, &state.vad_path, false) {
        log::warn!("model load: {e}");
    }
}

/// Checks every minute and unloads the model after `unload_after_idle_min`.
pub fn spawn_idle_unloader(state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        let Some(min) = state.settings_snapshot().unload_after_idle_min else {
            continue;
        };
        let idle_ms = now_ms() - state.last_used_ms.load(Ordering::Relaxed);
        if idle_ms < i64::from(min) * 60_000 {
            continue;
        }
        // try_lock: never wait on the controller while it is transcribing.
        if let Ok(mut guard) = state.speech.try_lock() {
            if guard.is_some() {
                let old = guard.take();
                drop(guard);
                drop(old);
                log::info!("model unloaded after {min} idle minutes");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_model_path_points_at_bin() {
        let root = Path::new("models");
        let w = models::find("whisper-large-v3-turbo-q5").unwrap();
        assert!(engine_path(root, &w).ends_with(
            Path::new("whisper-large-v3-turbo-q5").join("ggml-large-v3-turbo-q5_0.bin")
        ));
        let p = models::find("parakeet-tdt-0.6b-v2-int8").unwrap();
        assert_eq!(
            engine_path(root, &p),
            models::model_dir(root, "parakeet-tdt-0.6b-v2-int8")
        );
    }
}
