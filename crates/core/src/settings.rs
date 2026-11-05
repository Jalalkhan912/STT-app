use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub hotkey: HotkeySpec,
    pub input_device: Option<String>,
    pub mic_always_on: bool,
    pub active_model: String,
    pub cleanup: CleanupConfig,
    pub overlay_position: OverlayPosition,
    pub launch_at_login: bool,
    pub audio_retention: Retention,
    pub unload_after_idle_min: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySpec {
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupConfig {
    pub remove_fillers: bool,
    pub fillers: Vec<String>,
    pub replacements: Vec<(String, String)>,
    pub vocabulary: Vec<String>,
    pub capitalize_first: bool,
    pub trailing_space: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum OverlayPosition {
    #[default]
    BottomCenter,
    TopCenter,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum Retention {
    #[default]
    Forever,
    Days(u32),
    Never,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 1,
            hotkey: HotkeySpec::default(),
            input_device: None,
            mic_always_on: false,
            active_model: "parakeet-tdt-0.6b-v2-int8".to_string(),
            cleanup: CleanupConfig::default(),
            overlay_position: OverlayPosition::default(),
            launch_at_login: false,
            audio_retention: Retention::default(),
            unload_after_idle_min: None,
        }
    }
}

impl Default for HotkeySpec {
    fn default() -> Self {
        let key = if cfg!(target_os = "macos") {
            "AltRight"
        } else {
            "ControlRight"
        };
        HotkeySpec {
            keys: vec![key.to_string()],
        }
    }
}

impl Default for CleanupConfig {
    fn default() -> Self {
        CleanupConfig {
            remove_fillers: true,
            fillers: ["um", "uh", "er", "ah", "hmm"].map(String::from).to_vec(),
            replacements: vec![
                ("new paragraph".to_string(), "\n\n".to_string()),
                ("new line".to_string(), "\n".to_string()),
            ],
            vocabulary: Vec::new(),
            capitalize_first: true,
            trailing_space: true,
        }
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

impl Settings {
    /// Loads settings; a missing file yields defaults, a corrupt file is
    /// renamed to `<name>.bad-<unix_ms>` and defaults are returned.
    pub fn load(path: &Path) -> Settings {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Settings::default();
        };
        match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(_) => {
                let ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let _ = std::fs::rename(path, sibling_with_suffix(path, &format!(".bad-{ms}")));
                Settings::default()
            }
        }
    }

    /// Writes to `<name>.tmp` then renames over the real file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = sibling_with_suffix(path, ".tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }
}
