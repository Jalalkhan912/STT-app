use dictation_core::settings::*;
use tempfile::tempdir;

#[test]
fn missing_file_gives_defaults() {
    let d = tempdir().unwrap();
    let s = Settings::load(&d.path().join("settings.json"));
    assert_eq!(s, Settings::default());
    assert_eq!(s.active_model, "parakeet-tdt-0.6b-v2-int8");
    assert_eq!(s.audio_retention, Retention::Forever);
    assert_eq!(s.cleanup.fillers, ["um", "uh", "er", "ah", "hmm"]);
    assert!(!s.mic_always_on);
}

#[test]
fn default_hotkey_per_os() {
    let k = Settings::default().hotkey.keys;
    if cfg!(target_os = "macos") {
        assert_eq!(k, ["AltRight"])
    } else {
        assert_eq!(k, ["ControlRight"])
    }
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn save_then_load_roundtrips() {
    let d = tempdir().unwrap();
    let p = d.path().join("settings.json");
    let mut s = Settings::default();
    s.audio_retention = Retention::Days(7);
    s.cleanup.vocabulary = vec!["Kubernetes".into()];
    s.save(&p).unwrap();
    assert_eq!(Settings::load(&p), s);
}

#[test]
fn corrupt_file_is_backed_up_and_defaults_used() {
    let d = tempdir().unwrap();
    let p = d.path().join("settings.json");
    std::fs::write(&p, "{not json").unwrap();
    assert_eq!(Settings::load(&p), Settings::default());
    let backups: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("settings.json.bad-"))
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(d.path().join(&backups[0])).unwrap(),
        "{not json"
    );
}

#[test]
fn unknown_and_missing_fields_tolerated() {
    let d = tempdir().unwrap();
    let p = d.path().join("settings.json");
    std::fs::write(
        &p,
        r#"{"version":1,"launch_at_login":true,"future_field":5}"#,
    )
    .unwrap();
    let s = Settings::load(&p);
    assert!(s.launch_at_login);
    assert_eq!(s.cleanup, CleanupConfig::default());
}
