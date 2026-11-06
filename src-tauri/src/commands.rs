use crate::platform::permissions::{self, PermissionStatus};
use crate::state::{now_ms, AppState};
use crate::{models_mgr, paths};
use dictation_core::cleanup::clean;
use dictation_core::history::{Dictation, Status};
use dictation_core::models::{self, ModelInfo};
use dictation_core::settings::Settings;
use dictation_core::wav::read_wav;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

type S<'a> = State<'a, Arc<AppState>>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Makes the OS login-item state match the setting.
pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    let mgr = app.autolaunch();
    let current = mgr.is_enabled().unwrap_or(false);
    if current != enabled {
        let r = if enabled { mgr.enable() } else { mgr.disable() };
        if let Err(e) = r {
            log::warn!("autostart update failed: {e}");
        }
    }
}

// ---- settings ----

#[tauri::command]
pub fn get_settings(state: S) -> Result<Settings, String> {
    Ok(state.settings_snapshot())
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: S, s: Settings) -> Result<(), String> {
    std::fs::create_dir_all(paths::data_dir()).map_err(err)?;
    s.save(&paths::settings_path()).map_err(err)?;
    {
        let mut cfg = state.ctrl_config.write().unwrap_or_else(|e| e.into_inner());
        cfg.cleanup = s.cleanup.clone();
        cfg.retention = s.audio_retention.clone();
    }
    state.set_hotkey(&s.hotkey);
    let old = {
        let mut cur = state.settings.write().unwrap_or_else(|e| e.into_inner());
        std::mem::replace(&mut *cur, s.clone())
    };
    if old.active_model != s.active_model {
        models_mgr::spawn_load(state.inner().clone());
    }
    if old.launch_at_login != s.launch_at_login {
        apply_autostart(&app, s.launch_at_login);
    }
    if old.audio_retention != s.audio_retention {
        if let Err(e) = state
            .history
            .apply_retention(now_ms(), s.audio_retention.clone())
        {
            log::warn!("retention failed: {e}");
        }
    }
    // The hotkey listener reads `state.hotkey`, written above. Changes to the
    // input device or always-on mic apply after an app restart (v1).
    let _ = app.emit("settings-changed", &s);
    Ok(())
}

#[tauri::command]
pub fn list_input_devices() -> Result<Vec<String>, String> {
    Ok(crate::platform::recorder::list_input_devices())
}

// ---- history ----

#[tauri::command]
pub fn list_history(
    state: S,
    before_id: Option<String>,
    limit: u32,
) -> Result<Vec<Dictation>, String> {
    state.history.list(before_id.as_deref(), limit).map_err(err)
}

#[tauri::command]
pub fn search_history(state: S, query: String, limit: u32) -> Result<Vec<Dictation>, String> {
    state.history.search(&query, limit).map_err(err)
}

#[tauri::command]
pub fn delete_dictation(state: S, id: String) -> Result<(), String> {
    state.history.delete(&id).map_err(err)
}

fn retranscribe_blocking(state: &AppState, id: &str) -> Result<Dictation, String> {
    // The model may have been unloaded for idleness.
    models_mgr::ensure_loaded(state);
    let d = state
        .history
        .get(id)
        .map_err(err)?
        .ok_or_else(|| "dictation not found".to_string())?;
    let rel = d
        .audio_path
        .as_deref()
        .ok_or_else(|| "no audio kept for this dictation".to_string())?;
    let samples = read_wav(&state.history.abs_path(rel)).map_err(err)?;
    let cleanup = state
        .ctrl_config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .cleanup
        .clone();
    // The speech lock is held only for the inference itself.
    let (model, result) = {
        let mut guard = state.speech.lock().unwrap_or_else(|e| e.into_inner());
        let sp = guard
            .as_mut()
            .ok_or_else(|| "no model loaded".to_string())?;
        (sp.model_id(), sp.process(&samples))
    };
    state.touch();
    let raw = result.map_err(err)?.unwrap_or_default();
    let text = clean(&raw, &cleanup);
    let (raw, status) = if text.is_empty() {
        (String::new(), Status::NoSpeech)
    } else if matches!(d.status, Status::NoSpeech | Status::Error) {
        (raw, Status::CopiedOnly)
    } else {
        (raw, d.status)
    };
    state
        .history
        .update_transcript(id, &raw, &text, &model, status)
        .map_err(err)?;
    state
        .history
        .get(id)
        .map_err(err)?
        .ok_or_else(|| "dictation not found".to_string())
}

#[tauri::command]
pub async fn retranscribe(state: S<'_>, id: String) -> Result<Dictation, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || retranscribe_blocking(&st, &id))
        .await
        .map_err(err)?
}

#[tauri::command]
pub fn audio_file(state: S, id: String) -> Result<Option<String>, String> {
    let Some(d) = state.history.get(&id).map_err(err)? else {
        return Ok(None);
    };
    Ok(d.audio_path
        .map(|rel| state.history.abs_path(&rel))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn export_history(state: S<'_>, kind: String, path: String) -> Result<(), String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let out = Path::new(&path);
        let res = match kind.as_str() {
            "json" => st.history.export_json(out),
            "csv" => st.history.export_csv(out),
            "audio" => st.history.export_audio_zip(out),
            other => return Err(format!("unknown export kind {other}")),
        };
        res.map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn storage_usage(state: S) -> Result<u64, String> {
    state.history.storage_bytes().map_err(err)
}

// ---- models ----

#[derive(Serialize)]
pub struct ModelStatus {
    pub info: ModelInfo,
    pub installed: bool,
    pub active: bool,
}

#[derive(Serialize, Clone)]
struct ModelProgress {
    id: String,
    done: u64,
    total: u64,
}

#[derive(Serialize, Clone)]
struct ModelDone {
    id: String,
    error: Option<String>,
}

#[tauri::command]
pub fn list_models(state: S) -> Result<Vec<ModelStatus>, String> {
    let root = paths::models_dir();
    let active = state.settings_snapshot().active_model;
    Ok(models::catalog()
        .into_iter()
        .map(|info| ModelStatus {
            installed: models::is_installed(&root, &info),
            active: info.id == active,
            info,
        })
        .collect())
}

#[tauri::command]
pub fn download_model(app: AppHandle, state: S, id: String) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("unknown model {id}"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut dl = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if dl.contains_key(&id) {
            return Err("already downloading".into());
        }
        dl.insert(id.clone(), cancel.clone());
    }
    let st = state.inner().clone();
    std::thread::spawn(move || {
        let root = paths::models_dir();
        let progress_app = app.clone();
        let progress_id = id.clone();
        let result = models::install(
            &root,
            &m,
            &move |done, total| {
                let _ = progress_app.emit(
                    "model-progress",
                    ModelProgress {
                        id: progress_id.clone(),
                        done,
                        total,
                    },
                );
            },
            &cancel,
        );
        st.downloads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        let error = result.as_ref().err().map(|e| e.to_string());
        if let Some(e) = &error {
            log::warn!("download of {id} failed: {e}");
        }
        let _ = app.emit(
            "model-done",
            ModelDone {
                id: id.clone(),
                error,
            },
        );
        if result.is_ok() && st.settings_snapshot().active_model == id {
            models_mgr::spawn_load(st.clone());
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: S, id: String) -> Result<(), String> {
    if let Some(flag) = state
        .downloads
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn import_model(
    app: AppHandle,
    state: S<'_>,
    id: String,
    file_name: String,
    path: String,
) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("unknown model {id}"))?;
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        models::import_file(&paths::models_dir(), &m, &file_name, Path::new(&path)).map_err(err)?;
        if st.settings_snapshot().active_model == id
            && models::is_installed(&paths::models_dir(), &m)
        {
            models_mgr::spawn_load(st.clone());
        }
        let _ = app.emit("model-done", ModelDone { id, error: None });
        Ok(())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn set_active_model(app: AppHandle, state: S, id: String) -> Result<(), String> {
    let m = models::find(&id).ok_or_else(|| format!("unknown model {id}"))?;
    if !models::is_installed(&paths::models_dir(), &m) {
        return Err("model is not installed".into());
    }
    let updated = {
        let mut cur = state.settings.write().unwrap_or_else(|e| e.into_inner());
        cur.active_model = id;
        cur.clone()
    };
    updated.save(&paths::settings_path()).map_err(err)?;
    models_mgr::spawn_load(state.inner().clone());
    let _ = app.emit("settings-changed", &updated);
    Ok(())
}

// ---- permissions ----

#[tauri::command]
pub fn permission_status() -> Result<PermissionStatus, String> {
    Ok(permissions::status())
}

#[tauri::command]
pub fn open_permission_settings(kind: String) -> Result<(), String> {
    permissions::open_settings(&kind)
}

#[tauri::command]
pub fn request_permission(kind: String) -> Result<(), String> {
    permissions::request(&kind)
}
