use dictation_core::controller::{ControllerConfig, SharedSpeech};
use dictation_core::events::Event;
use dictation_core::history::History;
use dictation_core::settings::{HotkeySpec, Settings};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lifecycle of the speech model, shown in the UI.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ModelState {
    /// Nothing loaded (not installed, or unloaded for idleness).
    None,
    Loading,
    Ready,
    Error(String),
}

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub history: Arc<History>,
    pub speech: SharedSpeech,
    pub ctrl_config: Arc<RwLock<ControllerConfig>>,
    /// Sender for the controller; the hotkey listener (Task 11) clones this.
    pub events: Sender<Event>,
    /// Cancel flags of in-flight model downloads, by model id.
    pub downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub last_used_ms: AtomicI64,
    /// Path of the bundled Silero VAD model.
    pub vad_path: PathBuf,
    /// Live hotkey for the listener; every settings change writes it.
    pub hotkey: Arc<RwLock<HotkeySpec>>,
    /// Serialises model loads so two loaders never race to swap.
    pub load_lock: Mutex<()>,
    pub model_state: Mutex<ModelState>,
    /// Set once in `setup`; used to emit `model-state`.
    pub app: OnceLock<AppHandle>,
}

impl AppState {
    pub fn new(
        settings: Settings,
        history: Arc<History>,
        events: Sender<Event>,
        vad_path: PathBuf,
    ) -> Self {
        let ctrl_config = ControllerConfig {
            cleanup: settings.cleanup.clone(),
            retention: settings.audio_retention.clone(),
        };
        AppState {
            hotkey: Arc::new(RwLock::new(settings.hotkey.clone())),
            settings: RwLock::new(settings),
            history,
            speech: Arc::new(Mutex::new(None)),
            ctrl_config: Arc::new(RwLock::new(ctrl_config)),
            events,
            downloads: Mutex::new(HashMap::new()),
            last_used_ms: AtomicI64::new(now_ms()),
            vad_path,
            load_lock: Mutex::new(()),
            model_state: Mutex::new(ModelState::None),
            app: OnceLock::new(),
        }
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_hotkey(&self, spec: &HotkeySpec) {
        *self.hotkey.write().unwrap_or_else(|e| e.into_inner()) = spec.clone();
    }

    pub fn model_state(&self) -> ModelState {
        self.model_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Stores the model state and tells the UI.
    pub fn set_model_state(&self, s: ModelState) {
        {
            let mut cur = self.model_state.lock().unwrap_or_else(|e| e.into_inner());
            if *cur == s {
                return;
            }
            *cur = s.clone();
        }
        if let Some(app) = self.app.get() {
            let _ = app.emit("model-state", &s);
        }
    }

    /// Record activity for the idle-unload timer.
    pub fn touch(&self) {
        self.last_used_ms.store(now_ms(), Ordering::Relaxed);
    }
}
