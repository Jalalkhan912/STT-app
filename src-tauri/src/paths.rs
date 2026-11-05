use std::path::PathBuf;

/// `<platform data dir>/Dictation`, created if missing.
pub fn data_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Dictation");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn models_dir() -> PathBuf {
    let dir = data_dir().join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn logs_dir() -> PathBuf {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
