pub mod commands;
pub mod models_mgr;
pub mod overlay;
pub mod paths;
pub mod platform;
pub mod state;
pub mod tray;

use dictation_core::controller::{self, Controller};
use dictation_core::history::History;
use dictation_core::settings::Settings;
use overlay::{TauriOverlay, OVERLAY_LABEL};
use platform::hotkey_listener;
use platform::paste::SystemPaster;
use platform::recorder::CpalRecorder;
use state::{now_ms, AppState};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

fn spawn_retention(state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        let retention = state.settings_snapshot().audio_retention;
        match state.history.apply_retention(now_ms(), retention) {
            Ok(n) if n > 0 => log::info!("retention removed {n} recordings"),
            Ok(_) => {}
            Err(e) => log::warn!("retention failed: {e}"),
        }
        std::thread::sleep(Duration::from_secs(24 * 60 * 60));
    });
}

pub fn run() {
    // Only keyboard events are needed by the hotkey listener; the rdev fork
    // reads this when it builds the macOS event mask. Set before any thread
    // exists: mutating the environment later races with other threads.
    #[cfg(target_os = "macos")]
    std::env::set_var("KEYBOARD_ONLY", "1");

    let data_dir = paths::data_dir();
    let settings = Settings::load(&paths::settings_path());
    let history = match History::open(&data_dir) {
        Ok(h) => Arc::new(h),
        Err(e) => {
            eprintln!("cannot open history database: {e}");
            std::process::exit(1);
        }
    };

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(Target::new(TargetKind::Folder {
                    path: paths::logs_dir(),
                    file_name: Some("dictation".into()),
                }))
                .target(Target::new(TargetKind::Stdout))
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None::<Vec<&'static str>>,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::list_input_devices,
            commands::list_history,
            commands::search_history,
            commands::delete_dictation,
            commands::retranscribe,
            commands::audio_file,
            commands::export_history,
            commands::storage_usage,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::import_model,
            commands::set_active_model,
            commands::permission_status,
            commands::open_permission_settings,
            commands::request_permission,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            let vad_path = app
                .path()
                .resolve(
                    "resources/silero_vad_v4.onnx",
                    tauri::path::BaseDirectory::Resource,
                )
                .unwrap_or_else(|_| "resources/silero_vad_v4.onnx".into());
            let (tx, rx) = mpsc::channel();
            let state = Arc::new(AppState::new(settings, history, tx, vad_path));
            app.manage(state.clone());

            commands::apply_autostart(app.handle(), state.settings_snapshot().launch_at_login);
            spawn_retention(state.clone());
            models_mgr::spawn_load(state.clone());
            models_mgr::spawn_idle_unloader(state.clone());

            let boot = state.settings_snapshot();
            let overlay_app = app.handle().clone();
            let recorder = CpalRecorder::new(
                boot.input_device.clone(),
                boot.mic_always_on,
                Box::new(move |level| {
                    let _ = overlay_app.emit_to(OVERLAY_LABEL, "audio-level", level);
                }),
                state.events.clone(),
            );
            let press_state = state.clone();
            hotkey_listener::spawn(state.hotkey.clone(), state.events.clone(), move || {
                // Runs on the OS hook thread: no blocking work here.
                press_state.touch();
                let unloaded =
                    matches!(press_state.speech.try_lock(), Ok(guard) if guard.is_none());
                if unloaded {
                    let st = press_state.clone();
                    std::thread::spawn(move || models_mgr::ensure_loaded(&st));
                }
            });

            let ctrl = Controller::new(
                Box::new(recorder),
                state.speech.clone(),
                Box::new(SystemPaster::new()),
                Box::new(TauriOverlay::new(app.handle().clone(), state.clone())),
                state.history.clone(),
                state.ctrl_config.clone(),
                Box::new(now_ms),
            );
            std::thread::spawn(move || controller::run(ctrl, rx));

            overlay::setup(app.handle())?;
            tray::setup(app.handle())?;

            // First run (no model yet): open the main window for setup.
            let active = state.settings_snapshot().active_model;
            let installed = dictation_core::models::find(&active)
                .is_some_and(|m| dictation_core::models::is_installed(&paths::models_dir(), &m));
            let perms = platform::permissions::status();
            let perm_missing = cfg!(target_os = "macos")
                && !(perms.microphone && perms.accessibility && perms.input_monitoring);
            if !installed || perm_missing {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Dictation");
}
