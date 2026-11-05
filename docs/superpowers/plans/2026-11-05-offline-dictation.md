# Offline Dictation App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Tauri 2 desktop app for macOS and Windows. Hold a hotkey to record, release it, and the app transcribes the speech offline with Parakeet, cleans up the text, pastes it, and saves the audio and text to history.

**Architecture:** A Cargo workspace with two crates:
- `crates/core` (`dictation-core`) holds all logic behind traits, so it can be tested on headless Linux: settings, cleanup, hotkey tracking, capture buffer, WAV, history, models, VAD/chunking, the speech pipeline and the controller state machine.
- `src-tauri` (`dictation-app`) is the thin platform layer: cpal mic, rdev hotkey, enigo/arboard paste, windows, tray and commands.

`ui/` is a Svelte 5 + Vite frontend with two pages: `overlay.html` and `index.html`.

**Tech Stack:**
- Rust (stable): tauri 2.12, transcribe-rs 0.3.12 (features `onnx`, `vad-silero`, `whisper-cpp`), rusqlite 0.41 (`bundled`), cpal 0.18, rubato 5, rdev 0.5, enigo 0.6, arboard 3.7, hound 3.5, ulid 3, chrono, serde/serde_json, reqwest 0.13 (blocking, rustls), sha2 0.11, zip, thiserror.
- Dev-dependencies: tempfile, tiny_http.
- UI: Svelte 5 + Vite + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-05-offline-dictation-design.md`

## Global Constraints

- English only. Fully offline except model downloads the user starts.
- Audio inside the app: 16 kHz mono f32 in [-1, 1]. On disk: 16 kHz mono 16-bit WAV.
- Pre-roll: 200 ms. Tail after release: 150 ms. Minimum hold: 300 ms. Hard limit: 10 min. Chunking when > 2 min, chunks ≤ 30 s.
- Clipboard restored 300 ms after paste.
- Overlay pill 120 × 36 px, bottom-center by default. Fade in 120 ms, out 150 ms. Levels ~30 Hz.
- Default model id: `parakeet-tdt-0.6b-v2-int8`. Optional: `whisper-large-v3-turbo-q5`.
- Data folder `<OS app data>/Dictation/`, containing `models/`, `recordings/YYYY/MM/<ulid>.wav`, `history.db`, `settings.json`, `logs/`.
- Hotkey names are W3C `KeyboardEvent.code` strings (`AltRight`, `ControlRight`, `Space`, `KeyD`, `Escape`). Default: `AltRight` on macOS, `ControlRight` on Windows.
- History statuses: `pasted`, `copied_only`, `no_speech`, `error`.
- The dev server has ~2.8 GB free disk and no display or mic:
  - Locally, run only `cargo test -p dictation-core` (without engine features) and `npm test`.
  - `src-tauri` and the engine features are built and tested on GitHub Actions (macos-14, windows-latest).
- Workspace `default-members = ["crates/core"]`, so a bare `cargo test` never builds Tauri on Linux.

## Review Focus

1. **Corrupt or hand-edited `settings.json`:** the app must still start with defaults and keep a backup of the bad file. → Task 2 `corrupt_file_is_backed_up_and_defaults_used`
2. **Search text with FTS5 syntax characters** (`"`, `(`, `AND`, `*`): must return results or nothing, never an error. → Task 6 `search_tolerates_fts_syntax`
3. **Dictation that is only filler words** ("um, uh"): cleaned text is empty, so nothing is pasted and it's recorded as `no_speech`. → Task 9 `only_fillers_is_no_speech`
4. **Mic unplugged or failing mid-recording:** the overlay shows a mic error, nothing is pasted, and the controller returns to Idle and keeps accepting the next press. → Task 9 `recorder_failure_shows_mic_error_and_recovers`
5. **Interrupted or corrupted model download:** a resume continues from the `.part` file, and a checksum mismatch never leaves a usable-looking model file. → Task 7 `resumes_partial_download` and `checksum_mismatch_leaves_no_file`

---

### Task 1: Workspace scaffold, CI for core, spec amendments

**Files:**
- Create: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `.gitignore`, `rust-toolchain.toml`, `.github/workflows/ci.yml`
- Modify: `docs/superpowers/specs/2026-11-05-offline-dictation-design.md`

**Interfaces:**
- Produces: crate `dictation_core` with empty modules `settings, cleanup, events, hotkey, capture, wav, history, models, vad, engine, speech, controller`. Features:
  - `parakeet = ["transcribe-rs/onnx", "transcribe-rs/vad-silero"]`
  - `whisper = ["transcribe-rs/whisper-cpp"]`
  - `whisper-metal = ["whisper", "transcribe-rs/whisper-metal"]`
  - `whisper-vulkan = ["whisper", "transcribe-rs/whisper-vulkan"]`
  - None are default.

- [ ] **Step 1: Install the Rust toolchain (minimal profile, saves disk)**

Run: `curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --component clippy,rustfmt && . ~/.cargo/env && cargo --version`
Expected: prints `cargo 1.x`.

- [ ] **Step 2: Create the workspace**

Root `Cargo.toml`:
- `members = ["crates/core", "src-tauri"]`
- `default-members = ["crates/core"]`
- `resolver = "2"`

`src-tauri` won't exist until Task 10. Until then, list only `crates/core` in `members`; Task 10 adds `src-tauri`.

`rust-toolchain.toml`: `channel = "stable"`.

`.gitignore`: `target/`, `node_modules/`, `ui/dist/`, `*.part`, `crates/core/tests/models/`.

- [ ] **Step 3: Verify it builds**

Run: `cargo test -p dictation-core`
Expected: `test result: ok. 0 passed`.

- [ ] **Step 4: Add CI job `core`** in `.github/workflows/ci.yml`. It runs on `ubuntu-latest` on push/PR: `cargo fmt --check`, `cargo clippy -p dictation-core -- -D warnings`, `cargo test -p dictation-core`.

- [ ] **Step 5: Amend the spec** with the decisions made while planning. Add a "§11 Planning amendments" section:
  - Parakeet runs on CPU on every platform in v1. ORT GPU providers are deferred and revisited only if the Task 8 benchmark misses the ~1 s target.
  - Whisper GPU: Metal on macOS, **Vulkan** on Windows (works on NVIDIA without a CUDA toolkit; replaces the "optional CUDA download").
  - The `whisper-distil-large-v3.en` entry is removed: there's no maintained English-only distil GGML build. Turbo q5 remains.
  - Microphone: by default the stream opens on key down. Setting `mic_always_on` (default `false`) keeps it warm, which enables the 200 ms pre-roll; the OS mic indicator then stays on. With it off, pre-roll is whatever arrives after the stream opens.
  - VAD is Silero v4 (`silero_vad_v4.onnx`, 1 807 522 bytes, sha256 `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28`), bundled as a Tauri resource.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "chore: scaffold workspace, core crate, CI; amend spec"
```

---

### Task 2: Settings

**Files:**
- Create: `crates/core/src/settings.rs`
- Test: `crates/core/tests/settings.rs`

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, structs `#[serde(default)]`):
  - `pub struct Settings`, with fields:
    - `version: u32` (1)
    - `hotkey: HotkeySpec`
    - `input_device: Option<String>`
    - `mic_always_on: bool` (false)
    - `active_model: String`
    - `cleanup: CleanupConfig`
    - `overlay_position: OverlayPosition`
    - `launch_at_login: bool` (false)
    - `audio_retention: Retention`
    - `unload_after_idle_min: Option<u32>` (None)
  - `pub struct HotkeySpec { pub keys: Vec<String> }`
  - `pub struct CleanupConfig`, with fields:
    - `remove_fillers: bool`
    - `fillers: Vec<String>`
    - `replacements: Vec<(String, String)>`
    - `vocabulary: Vec<String>`
    - `capitalize_first: bool`
    - `trailing_space: bool`
  - `pub enum OverlayPosition { BottomCenter, TopCenter, BottomLeft, BottomRight }` (default `BottomCenter`)
  - `pub enum Retention { Forever, Days(u32), Never }` (default `Forever`)
  - `impl Settings { pub fn load(path: &Path) -> Settings; pub fn save(&self, path: &Path) -> std::io::Result<()> }`
  - `CleanupConfig::default()`:
    - fillers `["um","uh","er","ah","hmm"]`
    - replacements `[("new paragraph","\n\n"), ("new line","\n")]`
    - vocabulary `[]`
    - all three bools `true`

- [ ] **Step 1: Write failing tests**

```rust
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
    if cfg!(target_os = "macos") { assert_eq!(k, ["AltRight"]) } else { assert_eq!(k, ["ControlRight"]) }
}

#[test]
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
    let backups: Vec<_> = std::fs::read_dir(d.path()).unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("settings.json.bad-")).collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read_to_string(d.path().join(&backups[0])).unwrap(), "{not json");
}

#[test]
fn unknown_and_missing_fields_tolerated() {
    let d = tempdir().unwrap();
    let p = d.path().join("settings.json");
    std::fs::write(&p, r#"{"version":1,"launch_at_login":true,"future_field":5}"#).unwrap();
    let s = Settings::load(&p);
    assert!(s.launch_at_login);
    assert_eq!(s.cleanup, CleanupConfig::default());
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test settings`
Expected: compile error, because `Settings` isn't defined yet.

- [ ] **Step 3: Implement `settings.rs`**
  - `save` writes `settings.json.tmp`, then renames it over the real file.
  - On a parse error, `load` renames the file to `settings.json.bad-<unix_ms>`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test settings`
Expected: 5 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): settings with defaults, atomic save, corrupt-file backup"`

---

### Task 3: Text cleanup

**Files:**
- Create: `crates/core/src/cleanup.rs`
- Test: `crates/core/tests/cleanup.rs`

**Interfaces:**
- Consumes: `CleanupConfig` (Task 2).
- Produces: `pub fn clean(raw: &str, cfg: &CleanupConfig) -> String`. Returns `""` when nothing is left. Never adds a trailing space to an empty result.

Rule order follows spec §6:
1. trim and collapse whitespace
2. remove fillers
3. replacements, longest phrase first
4. vocabulary
5. capitalize the first letter
6. trailing space

Matching is whole-word and case-insensitive. Build the regexes with the `regex` crate and escape user input.
- Filler removal also eats one directly following `,` or `.` and the whitespace after it.
- Replacement matching eats a directly following `,` or `.`. When the replacement contains `\n`, it also removes spaces on both sides of the inserted text and capitalizes the first letter after a newline.

- [ ] **Step 1: Write failing tests**

```rust
use dictation_core::{cleanup::clean, settings::CleanupConfig};
fn c(s: &str) -> String { clean(s, &CleanupConfig::default()) }

#[test] fn removes_leading_filler() { assert_eq!(c("Um, I think we should go."), "I think we should go. "); }
#[test] fn removes_inner_filler() { assert_eq!(c("So, uh, let's start."), "So, let's start. "); }
#[test] fn filler_inside_word_untouched() { assert_eq!(c("The umbrella is here."), "The umbrella is here. "); }
#[test] fn only_fillers_becomes_empty() { assert_eq!(c("Um, uh."), ""); }
#[test] fn collapses_whitespace() { assert_eq!(c("  hello   world  "), "Hello world "); }
#[test] fn new_line_command() { assert_eq!(c("Hello. New line. world."), "Hello.\nWorld. "); }
#[test] fn new_paragraph_beats_new_line() { assert_eq!(c("One. New paragraph. Two."), "One.\n\nTwo. "); }
#[test] fn vocabulary_fixes_case() {
    let mut cfg = CleanupConfig::default();
    cfg.vocabulary = vec!["Kubernetes".into()];
    assert_eq!(clean("we deploy on kubernetes.", &cfg), "We deploy on Kubernetes. ");
}
#[test] fn user_replacement_with_regex_chars() {
    let mut cfg = CleanupConfig::default();
    cfg.replacements.push(("smiley face".into(), ":-)".into()));
    assert_eq!(clean("great smiley face", &cfg), "Great :-) ");
}
#[test] fn toggles_off() {
    let cfg = CleanupConfig { remove_fillers: false, capitalize_first: false, trailing_space: false, ..Default::default() };
    assert_eq!(clean("um hello", &cfg), "um hello");
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test cleanup`
Expected: compile error.

- [ ] **Step 3: Implement `clean`**

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test cleanup`
Expected: 10 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): rule-based text cleanup"`

---

### Task 4: Events and hotkey tracker

**Files:**
- Create: `crates/core/src/events.rs`, `crates/core/src/hotkey.rs`
- Test: `crates/core/tests/hotkey.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, PartialEq)] pub enum Event { Pressed { at_ms: u64 }, Released { at_ms: u64 }, Cancel, MaxDuration }`
  - `pub struct HotkeyTracker` with `new(spec: HotkeySpec)`, `set_spec(&mut self, spec: HotkeySpec)` and `on_key(&mut self, code: &str, down: bool, at_ms: u64) -> Option<Event>`

Tracker rules:
- **Active** = every key in the spec is held.
- `Pressed` fires on the transition into active. Auto-repeat key-downs produce nothing.
- `Released` fires when any spec key is released while active.
- `Escape` down while active → `Cancel`. The tracker then ignores the spec keys until all of them are released.

- [ ] **Step 1: Write failing tests**

```rust
use dictation_core::{events::Event, hotkey::HotkeyTracker, settings::HotkeySpec};
fn t(keys: &[&str]) -> HotkeyTracker { HotkeyTracker::new(HotkeySpec { keys: keys.iter().map(|s| s.to_string()).collect() }) }

#[test] fn single_key_press_release_ignores_repeat() {
    let mut h = t(&["AltRight"]);
    assert_eq!(h.on_key("AltRight", true, 1000), Some(Event::Pressed { at_ms: 1000 }));
    assert_eq!(h.on_key("AltRight", true, 1050), None);
    assert_eq!(h.on_key("AltRight", false, 2000), Some(Event::Released { at_ms: 2000 }));
}
#[test] fn combo_requires_all_keys() {
    let mut h = t(&["ControlLeft", "Space"]);
    assert_eq!(h.on_key("ControlLeft", true, 1), None);
    assert_eq!(h.on_key("Space", true, 2), Some(Event::Pressed { at_ms: 2 }));
    assert_eq!(h.on_key("ControlLeft", false, 3), Some(Event::Released { at_ms: 3 }));
    assert_eq!(h.on_key("Space", false, 4), None);
}
#[test] fn escape_cancels_and_swallows_release() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1);
    assert_eq!(h.on_key("Escape", true, 2), Some(Event::Cancel));
    assert_eq!(h.on_key("AltRight", false, 3), None);
    assert_eq!(h.on_key("AltRight", true, 4), Some(Event::Pressed { at_ms: 4 }));
}
#[test] fn escape_when_idle_does_nothing() { assert_eq!(t(&["AltRight"]).on_key("Escape", true, 1), None); }
#[test] fn unrelated_keys_ignored() {
    let mut h = t(&["AltRight"]);
    h.on_key("AltRight", true, 1);
    assert_eq!(h.on_key("KeyA", true, 2), None);
    assert_eq!(h.on_key("KeyA", false, 3), None);
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test hotkey`
Expected: compile error.

- [ ] **Step 3: Implement** using a `HashSet<String>` of held keys plus `active` and `suppressed` flags.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test hotkey`
Expected: 5 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): hotkey tracker"`

---

### Task 5: Capture buffer and WAV I/O

**Files:**
- Create: `crates/core/src/capture.rs`, `crates/core/src/wav.rs`
- Test: `crates/core/tests/capture.rs`

**Interfaces:**
- Produces:
  - `pub const SAMPLE_RATE: u32 = 16_000;`
  - `pub struct CaptureBuffer`, with methods:
    - `new(preroll_ms: u32, max_ms: u32)`
    - `push(&mut self, s: &[f32]) -> PushResult`
    - `start(&mut self)`
    - `take(&mut self) -> Vec<f32>`
    - `discard(&mut self)`
    - `is_recording(&self) -> bool`
  - `pub enum PushResult { Ok, MaxReached }`
  - `pub fn rms(s: &[f32]) -> f32`
  - `pub fn write_wav(path: &Path, s: &[f32]) -> std::io::Result<()>`
  - `pub fn read_wav(path: &Path) -> std::io::Result<Vec<f32>>`

Behavior:
- When not recording, `push` keeps only the last `preroll_ms` of samples.
- `start` begins recording. The recording starts with the pre-roll samples.
- `MaxReached` is returned once the recorded part (excluding pre-roll) reaches `max_ms`. Samples past the limit are dropped.
- `take` and `discard` stop recording and clear the buffer.
- `write_wav` creates parent directories, clamps samples to [-1, 1] and writes i16.

- [ ] **Step 1: Write failing tests**

```rust
use dictation_core::{capture::*, wav::*};

#[test] fn keeps_only_preroll_when_idle() {
    let mut b = CaptureBuffer::new(200, 60_000);
    b.push(&vec![0.1; 10_000]);
    b.start();
    b.push(&vec![0.5; 100]);
    let s = b.take();
    assert_eq!(s.len(), 3_300);
    assert!(s[..3_200].iter().all(|&x| x == 0.1) && s[3_200..].iter().all(|&x| x == 0.5));
    assert!(!b.is_recording());
}
#[test] fn reports_max_and_caps_length() {
    let mut b = CaptureBuffer::new(0, 1_000);
    b.start();
    assert_eq!(b.push(&vec![0.0; 15_999]), PushResult::Ok);
    assert_eq!(b.push(&vec![0.0; 10]), PushResult::MaxReached);
    assert_eq!(b.take().len(), 16_000);
}
#[test] fn discard_clears() {
    let mut b = CaptureBuffer::new(200, 1_000);
    b.start(); b.push(&[0.3; 50]); b.discard();
    b.start(); assert!(b.take().is_empty());
}
#[test] fn rms_value() { assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6); assert_eq!(rms(&[]), 0.0); }
#[test] fn wav_roundtrip_and_clamp() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a/b/x.wav");
    write_wav(&p, &[0.0, 0.5, -0.5, 2.0]).unwrap();
    let spec = hound::WavReader::open(&p).unwrap().spec();
    assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (16_000, 1, 16));
    let r = read_wav(&p).unwrap();
    assert_eq!(r.len(), 4);
    assert!((r[1] - 0.5).abs() < 1e-3 && (r[3] - 1.0).abs() < 1e-3);
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test capture`
Expected: compile error.

- [ ] **Step 3: Implement.** Use a `VecDeque<f32>` for the pre-roll ring.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test capture`
Expected: 5 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): capture buffer with pre-roll, WAV I/O"`

---

### Task 6: History store

**Files:**
- Create: `crates/core/src/history.rs`
- Test: `crates/core/tests/history.rs`

**Interfaces:**
- Consumes: `write_wav`, `Retention`.
- Produces:
  - `#[derive(Serialize, Clone, Copy, PartialEq, Debug)] #[serde(rename_all = "snake_case")] pub enum Status { Pasted, CopiedOnly, NoSpeech, Error }`
  - `pub struct NewDictation`, with fields:
    - `created_at_ms: i64`
    - `duration_ms: i64`
    - `text: String`
    - `raw_text: String`
    - `model: String`
    - `target_app: Option<String>`
    - `status: Status`
  - `#[derive(Serialize, Clone, Debug, PartialEq)] pub struct Dictation`: the same fields plus `id: String` and `audio_path: Option<String>`. `audio_path` is relative to the data dir, e.g. `recordings/2026/11/<id>.wav`.
  - `impl History`, with methods:
    - `open(data_dir: &Path) -> Result<History>`
    - `add(&self, d: NewDictation, audio: Option<&[f32]>) -> Result<Dictation>`
    - `get(&self, id: &str) -> Result<Option<Dictation>>`
    - `list(&self, before_id: Option<&str>, limit: u32) -> Result<Vec<Dictation>>` (newest first)
    - `search(&self, q: &str, limit: u32) -> Result<Vec<Dictation>>`
    - `update_transcript(&self, id: &str, raw: &str, text: &str, model: &str, status: Status) -> Result<()>`
    - `delete(&self, id: &str) -> Result<()>`
    - `apply_retention(&self, now_ms: i64, r: Retention) -> Result<u32>` (returns the number of audio files removed)
    - `storage_bytes(&self) -> Result<u64>` (total size of `recordings/`)
    - `export_json(&self, out: &Path)`, `export_csv(&self, out: &Path)`, `export_audio_zip(&self, out: &Path)`, each `-> Result<()>`
    - `abs_path(&self, rel: &str) -> PathBuf`
  - `pub type Result<T> = std::result::Result<T, HistoryError>`, where `HistoryError` is a thiserror enum wrapping `rusqlite::Error` and `io::Error`.

Implementation decisions:
- The ID is `Ulid::from_datetime(created_at)`. The recording folder comes from `created_at` in UTC.
- Use FTS5 external-content table `dictations_fts(text)` with insert, update and delete triggers. Schema version goes in `PRAGMA user_version = 1`.
- **Search sanitizing:** split the query on whitespace, wrap each token in double quotes (doubling any inner `"`), and add `*` to the last token. An empty query returns `list(None, limit)`.
- **Retention:**
  - `Days(n)`: delete audio files for rows with `created_at < now - n days`.
  - `Never`: delete all audio files.
  - Set `audio_path = NULL` on affected rows. A file that's already missing is not an error.
- CSV header: `id,created_at,duration_ms,text,raw_text,model,target_app,status`. Quote fields per RFC 4180.

- [ ] **Step 1: Write failing tests**

Helper: `fn nd(ms: i64, text: &str) -> NewDictation` with `status: Pasted`, `model: "m"`, `raw_text: text`, `duration_ms: 1000`, `target_app: None`.

```rust
#[test] fn add_with_audio_writes_file() {
    let d = tempdir().unwrap(); let h = History::open(d.path()).unwrap();
    let x = h.add(nd(1_762_300_000_000, "hello"), Some(&[0.1; 1600])).unwrap();
    let rel = x.audio_path.clone().unwrap();
    assert!(rel.starts_with("recordings/2025/11/") && rel.ends_with(&format!("{}.wav", x.id)));
    assert!(h.abs_path(&rel).exists());
    assert_eq!(h.get(&x.id).unwrap(), Some(x));
}
#[test] fn list_newest_first_and_paginates() {
    // add at ms 1000, 2000, 3000 → list(None,2) = [3000,2000]; list(Some(id_of_2000),2) = [1000]
}
#[test] fn search_prefix_and_word() {
    // add "We deploy on Kubernetes" and "Lunch at noon" → search("kube",10) has 1 item with that text; search("noon",10) 1 item
}
#[test] fn search_tolerates_fts_syntax() {
    let d = tempdir().unwrap(); let h = History::open(d.path()).unwrap();
    h.add(nd(1, r#"he said "hi" (AND) more"#), None).unwrap();
    for q in [r#"he said "hi"#, "AND (", "*", "\"", "NEAR(a b)", ""] { assert!(h.search(q, 10).is_ok(), "{q}"); }
    assert_eq!(h.search("(AND)", 10).unwrap().len(), 1);
}
#[test] fn delete_removes_row_file_and_index() { /* add with audio; delete; get==None; file gone; search finds 0 */ }
#[test] fn update_transcript_reindexes() { /* add "alpha"; update to "beta", status Error; search("beta")==1, search("alpha")==0, get().status==Error */ }
#[test] fn retention_days_and_never() {
    // now = 100 days in ms; add old (now - 8d) and new (now - 1d), both with audio.
    // apply_retention(now, Days(7)) == 1; old.audio_path None + file gone; new unchanged.
    // manually delete new's file from disk; apply_retention(now, Never) == 0 and Ok; new.audio_path None.
}
#[test] fn storage_bytes_counts_recordings() { /* add with 16000 samples → storage_bytes >= 32_000 */ }
#[test] fn exports() {
    // add text "a, \"b\"\nc" with audio, plus one without
    // export_json → serde_json array len 2
    // export_csv → first line == header; content contains "\"a, \"\"b\"\"\nc\""
    // export_audio_zip → zip archive len 1
}
```

Comments in the tests above give the exact assertions the implementer writes out.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test history`
Expected: compile error.

- [ ] **Step 3: Implement `history.rs`.** Use `rusqlite` with feature `bundled` (includes FTS5), plus `zip`, `ulid` and `chrono`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test history`
Expected: 9 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): SQLite history with FTS, retention, export"`

---

### Task 7: Model catalog, download, verify, import

**Files:**
- Create: `crates/core/src/models.rs`
- Test: `crates/core/tests/models.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Debug, Serialize)] pub enum EngineKind { Parakeet, Whisper }`
  - `#[derive(Clone, Debug, Serialize)] pub struct ModelFile { pub name: String, pub url: String, pub sha256: String, pub size: u64 }`
  - `#[derive(Clone, Debug, Serialize)] pub struct ModelInfo { pub id: String, pub label: String, pub kind: EngineKind, pub files: Vec<ModelFile> }`
  - `pub fn catalog() -> Vec<ModelInfo>`
  - `pub fn find(id: &str) -> Option<ModelInfo>`
  - `pub fn model_dir(models_root: &Path, id: &str) -> PathBuf` (`models_root/id`)
  - `pub fn is_installed(models_root: &Path, m: &ModelInfo) -> bool` (all files present with the expected size; no hashing)
  - `pub fn install(models_root: &Path, m: &ModelInfo, progress: &dyn Fn(u64, u64), cancel: &AtomicBool) -> Result<(), ModelError>`
  - `pub fn import_file(models_root: &Path, m: &ModelInfo, file_name: &str, src: &Path) -> Result<(), ModelError>`
  - `pub enum ModelError { Http(String), Io(io::Error), ChecksumMismatch { file: String }, Cancelled, UnknownFile(String) }`

Catalog values (URLs use the `https://huggingface.co/<repo>/resolve/main/<file>` form):

| id | label | file | size | sha256 |
|---|---|---|---|---|
| `parakeet-tdt-0.6b-v2-int8` | Parakeet TDT 0.6B v2 (English, recommended) | `encoder-model.int8.onnx` | 652184014 | `3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155` |
| ″ | ″ | `decoder_joint-model.int8.onnx` | 8998286 | `a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3` |
| ″ | ″ | `nemo128.onnx` | 139764 | `a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f` |
| ″ | ″ | `vocab.txt` | 9384 | `f9e52fa25130a1285be756011a6e5cbd064810781816832581d76a6d7dc31734` |
| `whisper-large-v3-turbo-q5` | Whisper large-v3 turbo (GPU) | `ggml-large-v3-turbo-q5_0.bin` | 574041195 | `394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2` |

The Parakeet repo is `istupakov/parakeet-tdt-0.6b-v2-onnx`. The Whisper repo is `ggerganov/whisper.cpp`.

Download algorithm for each file:
1. If the final file exists with the right size, skip it.
2. Otherwise download to `<name>.part`, sending `Range: bytes=<part_len>-` if the `.part` file exists. Accept `206` (append) or `200` (truncate and restart).
3. Report `progress(done_all_files, total_all_files)` at least every 1 MB.
4. Check `cancel` between reads. If cancelled, return `Cancelled` and keep the `.part` file.
5. When the download finishes, compute the SHA-256 of the `.part` file. If it doesn't match, delete `.part` and return `ChecksumMismatch`. If it matches, rename it to the final name.

`import_file` copies to `<name>.part`, verifies the hash, then renames. An unknown file name returns `UnknownFile`.

- [ ] **Step 1: Write failing tests**

Test helper `serve(body: Vec<u8>) -> (String /*base url*/, Arc<Mutex<Vec<Option<String>>>> /*Range headers seen*/)`: a `tiny_http` server on `127.0.0.1:0`, running on a thread, that honors `Range: bytes=N-` with a 206 response.

`fn info(base: &str, body: &[u8]) -> ModelInfo` builds one file `f.bin` whose sha256 is computed from `body`.

```rust
#[test] fn catalog_has_default_parakeet() {
    let m = find("parakeet-tdt-0.6b-v2-int8").unwrap();
    assert_eq!(m.kind, EngineKind::Parakeet);
    assert_eq!(m.files.len(), 4);
    assert!(find("whisper-large-v3-turbo-q5").is_some());
}
#[test] fn installs_and_reports_installed() { /* 3 MB body; install ok; is_installed true; last progress == (len,len) */ }
#[test] fn resumes_partial_download() {
    // write first 1 MB of body to models_root/<id>/f.bin.part; install; final bytes == body;
    // ranges seen == [Some("bytes=1048576-")]
}
#[test] fn checksum_mismatch_leaves_no_file() {
    // info with wrong sha; install → Err(ChecksumMismatch{file:"f.bin"}); neither f.bin nor f.bin.part exists; is_installed false
}
#[test] fn cancel_keeps_part() { /* cancel = AtomicBool::new(true) → Err(Cancelled); f.bin absent */ }
#[test] fn import_verifies() { /* import good src ok + installed; import wrong-content src → ChecksumMismatch; import "x.bin" → UnknownFile */ }
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test models`
Expected: compile error.

- [ ] **Step 3: Implement** with `reqwest::blocking` (features `rustls-tls`, `blocking`; default features off) and `sha2`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test models`
Expected: 6 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat(core): model catalog with resumable verified download and import"`

---

### Task 8: VAD chunking, speech pipeline, engine adapters, accuracy benchmark

**Files:**
- Create: `crates/core/src/vad.rs`, `crates/core/src/engine.rs`, `crates/core/src/speech.rs`
- Create: `crates/core/tests/speech.rs`, `crates/core/tests/accuracy.rs`, `crates/core/tests/fixtures/jfk.wav`, `crates/core/tests/fixtures/jfk.txt`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Produces:
  - `pub trait SpeechDetector: Send { fn frame_size(&self) -> usize; fn is_speech(&mut self, frame: &[f32]) -> Result<bool, SpeechError>; fn reset(&mut self); }`
  - `pub fn speech_frames(det: &mut dyn SpeechDetector, s: &[f32]) -> Result<Vec<bool>, SpeechError>` (the last partial frame is zero-padded)
  - `pub fn has_speech(flags: &[bool]) -> bool` (true when at least 8 frames are speech)
  - `pub fn plan_chunks(flags: &[bool], frame: usize, total: usize, max_chunk: usize) -> Vec<Range<usize>>`
  - `pub trait Engine: Send { fn id(&self) -> &str; fn transcribe(&mut self, s: &[f32]) -> Result<String, SpeechError>; }`
  - `pub trait Speech: Send { fn model_id(&self) -> String; fn process(&mut self, s: &[f32]) -> Result<Option<String>, SpeechError>; }` (`None` means no speech)
  - `pub struct SpeechPipeline { engine: Box<dyn Engine>, detector: Box<dyn SpeechDetector> }`, with `new(..)`, implementing `Speech`
  - `pub enum SpeechError { Engine(String), Vad(String) }` (thiserror)
  - Behind feature `parakeet`:
    - `pub struct ParakeetEngine` with `load(dir: &Path) -> Result<Self, SpeechError>`, wrapping `transcribe_rs::onnx::parakeet::ParakeetModel::load(dir, &Quantization::Int8)` and `SpeechModel::transcribe(&samples, &TranscribeOptions::default())`
    - `pub struct SileroDetector` with `load(path: &Path) -> Result<Self, SpeechError>`, wrapping `transcribe_rs::vad::SileroVad::new(path, 0.3)` (frame size 480)
  - Behind feature `whisper`: `pub struct WhisperEngine` with `load(file: &Path) -> Result<Self, SpeechError>`, wrapping `transcribe_rs::whisper_cpp::WhisperEngine::load` and `transcribe_with(&s, &WhisperInferenceParams { language: Some("en".into()), ..Default::default() })` (check the param field name against the 0.3.12 source)
  - `pub fn warm_up(e: &mut dyn Engine) -> Result<(), SpeechError>` transcribes 1 s of silence.

`SpeechPipeline::process` (see the sketch after this list):
1. Get `flags = speech_frames(..)`.
2. If not `has_speech`, reset the detector and return `None`.
3. If `s.len() <= 1_920_000` (2 min), use one chunk; otherwise use `plan_chunks(flags, frame, len, 480_000)`.
4. Transcribe each chunk, trim the texts, join the non-empty ones with `" "`, then reset the detector.
5. An empty joined text returns `None`.

```rust
// plan_chunks: greedy. cursor = 0; while total - cursor > max_chunk:
//   window = frames covering [cursor, cursor + max_chunk);
//   pick the longest run of non-speech frames in the window's last 50%;
//   cut = midpoint of that run (in samples); if there is no run, cut = cursor + max_chunk;
//   push cursor..cut; cursor = cut.  Finally push cursor..total.
```

- [ ] **Step 1: Write failing tests** in `tests/speech.rs`, using fakes:
  - `FakeDetector { frame: 480 }`: `is_speech` = `rms(frame) > 0.01`.
  - `FakeEngine` records each chunk length into an `Arc<Mutex<Vec<usize>>>` and returns `"chunk{n}"`.

```rust
#[test] fn silence_is_no_speech() { assert_eq!(pipeline().process(&vec![0.0; 32_000]).unwrap(), None); }
#[test] fn short_speech_is_single_chunk() {
    let (mut p, lens) = pipeline_with_log();
    assert_eq!(p.process(&vec![0.2; 48_000]).unwrap(), Some("chunk1".into()));
    assert_eq!(*lens.lock().unwrap(), vec![48_000]);
}
#[test] fn under_8_speech_frames_is_no_speech() {
    let mut s = vec![0.0; 32_000]; for x in &mut s[..480 * 7] { *x = 0.2 }
    assert_eq!(pipeline().process(&s).unwrap(), None);
}
#[test] fn long_audio_chunks_at_silence_under_30s() {
    // 150 s audio: speech everywhere except silent gaps (0.0) of 1 s starting at 25 s, 52 s, 80 s, 110 s, 135 s
    // every chunk len <= 480_000; chunk lens sum == total; first cut lies inside the 25–26 s gap (400_000..416_000)
}
#[test] fn no_silence_forces_hard_cuts() { /* 150 s all speech → chunk lens == [480000, 480000, 480000, 480000, 480000] */ }
#[test] fn empty_engine_text_is_no_speech() { /* FakeEngine returning "  " → None */ }
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test speech`
Expected: compile error.

- [ ] **Step 3: Implement `vad.rs`, `speech.rs` and `engine.rs`.** Put the adapters behind `#[cfg(feature = ...)]`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p dictation-core --test speech`
Expected: 6 passed.

- [ ] **Step 5: Add the accuracy and benchmark test**

Fixtures:
- `tests/fixtures/jfk.wav`: from `https://github.com/ggml-org/whisper.cpp/raw/master/samples/jfk.wav` (16 kHz mono; public-domain speech).
- `jfk.txt`: `And so my fellow Americans, ask not what your country can do for you, ask what you can do for your country.`

`tests/accuracy.rs`:
- Gated by `#![cfg(feature = "parakeet")]`. The test is marked `#[ignore]`.
- Reads model dir `$DICTATION_MODELS/parakeet-tdt-0.6b-v2-int8`.
- For every `fixtures/*.wav` with a matching `.txt`: transcribes it and computes WER. Normalization: lowercase, strip `[^a-z0-9' ]`, split on whitespace; Levenshtein distance over words ÷ number of reference words.
- Prints `WER <file>: x.xx%  RTF: y.yyy`.
- Asserts mean WER `< 0.08`.

The user can add more fixture clips later.

- [ ] **Step 6: Add CI job `engine-accuracy`** on `macos-14`:
  - Cache `~/models` keyed by the model id.
  - Download the 4 Parakeet files via `curl` into `~/models/parakeet-tdt-0.6b-v2-int8/` if they're missing.
  - Run `DICTATION_MODELS=~/models cargo test -p dictation-core --features parakeet --test accuracy -- --ignored --nocapture`.

Expected in CI: `WER jfk.wav: ≤ 8%`, the RTF printed, and the job passes.

- [ ] **Step 7: Commit**

`git commit -am "feat(core): VAD chunking, speech pipeline, Parakeet/Whisper adapters, accuracy test"`

---

### Task 9: Controller state machine

**Files:**
- Create: `crates/core/src/controller.rs`
- Test: `crates/core/tests/controller.rs`

**Interfaces:**
- Consumes: `Event` (Task 4), `Speech` (Task 8), `History`, `NewDictation`, `Status` (Task 6), `clean` (Task 3), `CleanupConfig`, `Retention` (Task 2).
- Produces:
  - `pub trait Recorder: Send { fn start(&mut self) -> Result<(), String>; fn stop(&mut self) -> Result<Vec<f32>, String>; fn cancel(&mut self); }`. `stop` includes the pre-roll and the 150 ms tail.
  - `pub enum PasteOutcome { Pasted, CopiedOnly }`
  - `pub trait Paster: Send { fn focused_app(&self) -> Option<String>; fn paste(&mut self, text: &str) -> PasteOutcome; }`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(tag = "kind", content = "message", rename_all = "snake_case")] pub enum OverlayState { Hidden, Recording, Processing, Done, NoSpeech, Copied, MicError(String), NoModel, Error(String) }`
  - `pub trait Overlay: Send { fn set(&self, s: OverlayState); }`
  - `pub type SharedSpeech = Arc<Mutex<Option<Box<dyn Speech>>>>`
  - `pub struct ControllerConfig { pub cleanup: CleanupConfig, pub retention: Retention }`
  - `pub struct Controller`, with:
    - `new(recorder: Box<dyn Recorder>, speech: SharedSpeech, paster: Box<dyn Paster>, overlay: Box<dyn Overlay>, history: Arc<History>, config: Arc<RwLock<ControllerConfig>>, now_ms: Box<dyn Fn() -> i64 + Send>)`
    - `handle(&mut self, ev: Event)`
  - `pub fn run(ctrl: Controller, rx: Receiver<Event>)` processes events. After handling a `Released` or `MaxDuration` that ran the pipeline, it drains and drops every queued `Pressed` and `Released`.

Behavior (spec §3):

| State + event | Action |
|---|---|
| Idle + Pressed | If speech is `None`: `NoModel`, stay Idle. `recorder.start()` error → `MicError`. Otherwise remember `focused_app` and the press time, then `Recording` |
| Recording + Cancel | `recorder.cancel()`, `Hidden`, Idle; nothing saved |
| Recording + Released, held < 300 ms | Cancel the same way, nothing saved |
| Recording + Released/MaxDuration | `Processing`, `stop()`. A stop error → `MicError`, nothing saved, Idle |
| Speech returns `None` | `NoSpeech`; save row with status `no_speech`, empty text, no audio |
| Speech returns `Err` | `Error(msg)`; save row with status `error` and the audio |
| Text returned, `clean` is empty | Same as `None` |
| Text returned, `clean` non-empty | `paste` → `Done` or `Copied`; save row with `pasted` or `copied_only` |

Saving rules:
- Audio is saved unless retention is `Never`.
- `duration_ms` = samples / 16.
- `model` = `speech.model_id()`.
- `target_app` = the focused app recorded on press.

- [ ] **Step 1: Write failing tests**

Fakes:
- `FakeRecorder { samples, fail_start, fail_stop }` logs its calls.
- `FakeSpeech { reply: Result<Option<String>, SpeechError> }`.
- `FakePaster { outcome }` logs pasted strings.
- `FakeOverlay` logs states in an `Arc<Mutex<Vec<OverlayState>>>`.
- History is real, in a tempdir. The clock is fixed.

```rust
#[test] fn happy_path_pastes_cleaned_text_and_saves() {
    // speech → "um, hello world." ; Pressed@0, Released@1000
    // pasted == ["Hello world. "]; overlay == [Recording, Processing, Done]
    // history.list: 1 row, status Pasted, text "Hello world. ", raw_text "um, hello world.", audio_path Some, target_app Some("TestApp")
}
#[test] fn short_tap_cancels() { /* Pressed@0, Released@200 → recorder.cancel called; overlay [Recording, Hidden]; history empty; nothing pasted */ }
#[test] fn escape_cancels() { /* Pressed, Cancel → same as above */ }
#[test] fn no_model_shows_message() { /* speech None → overlay [NoModel]; recorder.start not called */ }
#[test] fn no_speech_saves_row_without_audio() { /* reply Ok(None) → overlay ends NoSpeech; row status NoSpeech, audio_path None; nothing pasted */ }
#[test] fn only_fillers_is_no_speech() { /* reply Ok(Some("Um, uh.")) → NoSpeech; nothing pasted; row NoSpeech */ }
#[test] fn engine_error_keeps_audio() { /* reply Err → overlay ends Error(_); row Error with audio_path Some; nothing pasted */ }
#[test] fn copied_only_when_paste_fails() { /* paster CopiedOnly → overlay ends Copied; row CopiedOnly */ }
#[test] fn retention_never_skips_audio() { /* config retention Never → row audio_path None */ }
#[test] fn recorder_failure_shows_mic_error_and_recovers() {
    // fail_stop = true: Pressed@0, Released@1000 → overlay ends MicError(_), history empty, nothing pasted;
    // then fail_stop=false; Pressed@2000, Released@3000 → Done (controller accepted new press)
}
#[test] fn max_duration_processes_like_release() { /* Pressed@0, MaxDuration → Done, row saved */ }
#[test] fn run_drops_presses_queued_during_processing() {
    // send [Pressed@0, Released@1000, Pressed@1100, Released@2000] then drop the sender; run(); exactly 1 row and 1 paste
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p dictation-core --test controller`
Expected: compile error.

- [ ] **Step 3: Implement `controller.rs`.** Use `enum State { Idle, Recording { started_ms: u64, app: Option<String> } }`.

- [ ] **Step 4: Run the whole core suite**

Run: `cargo test -p dictation-core`
Expected: all tests pass (settings 5, cleanup 10, hotkey 5, capture 5, history 9, models 6, speech 6, controller 12).

- [ ] **Step 5: Commit**

`git commit -am "feat(core): dictation controller state machine"`

---

### Task 10: Tauri app shell, state wiring, commands, CI build

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/Info.plist`, `src-tauri/icons/*`
- Create: `src-tauri/resources/silero_vad_v4.onnx`
- Create: `src-tauri/src/{main.rs, lib.rs, paths.rs, state.rs, models_mgr.rs, commands.rs, tray.rs}`
- Create: `ui/package.json`, `ui/vite.config.ts`, `ui/index.html`, `ui/overlay.html`, `ui/src/main/main.ts`, `ui/src/overlay/main.ts` (placeholders filled in by Tasks 12–13)
- Modify: root `Cargo.toml` (add `src-tauri` to `members`), `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: everything in `dictation_core`.
- `dictation-app` features:
  - macOS target: `dictation-core/parakeet`, `dictation-core/whisper-metal`
  - Windows target: `dictation-core/parakeet`, `dictation-core/whisper-vulkan`
  - Set these as target-specific dependency features.
- Produces:
  - **`paths.rs`**: `pub fn data_dir() -> PathBuf` (`dirs::data_dir()/Dictation`, created if missing). Plus `models_dir()`, `settings_path()`, `logs_dir()`.
  - **`state.rs`**: `pub struct AppState` holding:
    - `settings: RwLock<Settings>`
    - `history: Arc<History>`
    - `speech: SharedSpeech`
    - `ctrl_config: Arc<RwLock<ControllerConfig>>`
    - `events: Sender<Event>`
    - `downloads: Mutex<HashMap<String, Arc<AtomicBool>>>`
    - `last_used_ms: AtomicI64`
  - **`models_mgr.rs`**: `pub fn load_active(state: &AppState, vad_path: &Path) -> Result<(), String>`
    - Builds `ParakeetEngine` or `WhisperEngine` from the catalog entry (Whisper uses `model_dir/<file>`).
    - Runs `warm_up`, wraps it in `SpeechPipeline` with `SileroDetector`, and stores it in `speech`.
    - Runs on a background thread at startup and whenever `active_model` changes.
    - An idle-unload thread checks every minute and sets `speech = None` when `unload_after_idle_min` has passed since `last_used_ms`. The next Pressed then gets `NoModel`, so the hotkey handler calls `load_active` synchronously if speech is `None` and a model is installed, before sending Pressed.

**Tauri commands** (`commands.rs`, all returning `Result<T, String>`):
- Settings:
  - `get_settings() -> Settings`
  - `set_settings(s: Settings)`: saves, updates `ctrl_config` and the hotkey spec, and reloads the model if `active_model` changed
  - `list_input_devices() -> Vec<String>`
- History:
  - `list_history(before_id: Option<String>, limit: u32)`
  - `search_history(query: String, limit: u32)`
  - `delete_dictation(id)`
  - `retranscribe(id) -> Dictation`: reads the WAV, runs `speech.process` and `clean`, then calls `update_transcript`
  - `audio_file(id) -> Option<String>`: absolute path, played through `convertFileSrc`
  - `export_history(kind: "json"|"csv"|"audio", path: String)`
  - `storage_usage() -> u64`
- Models:
  - `list_models() -> Vec<ModelStatus { info: ModelInfo, installed: bool, active: bool }>`
  - `download_model(id)`: spawns a thread and emits `model-progress` `{ id, done, total }`, then `model-done` `{ id, error: Option<String> }`
  - `cancel_download(id)`
  - `import_model(id, file_name, path)`
  - `set_active_model(id)`
- Permissions:
  - `permissions_status() -> { microphone: bool, accessibility: bool }` (Task 11 provides the implementation; stub returns `true` until then)
  - `open_permission_settings(kind: String)`

**Startup in `lib.rs`:**
1. Load settings and open history.
2. Run `apply_retention` now and every 24 h on a thread.
3. Start the model load.
4. Create the event channel and spawn `controller::run` on its own thread (ports from Task 11/12; until then use stub ports that log).
5. Set up the tray. Hide the main window on close instead of quitting.

**Plugins:** `tauri-plugin-autostart` (driven by `launch_at_login`), `tauri-plugin-dialog` (export/import file pickers), `tauri-plugin-single-instance`, `tauri-plugin-log` (writes to `logs_dir()`, rotating at 5 MB, keeping 3 files; never logs transcript text).

**`tauri.conf.json`:**
- `app.macOSPrivateApi: true` (transparent overlay)
- Windows:
  - `main`: 900 × 640, `visible: false`
  - `overlay`: `overlay.html`, 120 × 36, `decorations: false`, `transparent: true`, `alwaysOnTop: true`, `skipTaskbar: true`, `focus: false`, `visible: false`, `resizable: false`, `shadow: false`
- Asset protocol scope: `$DATA/Dictation/recordings/**`
- Bundle resource: `resources/silero_vad_v4.onnx`
- `Info.plist`: `NSMicrophoneUsageDescription` = "Dictation records your voice while you hold the shortcut so it can type what you say."

- [ ] **Step 1: Add the Silero resource and verify its checksum**

Run: `curl -sL -o src-tauri/resources/silero_vad_v4.onnx https://github.com/snakers4/silero-vad/raw/v4.0/files/silero_vad.onnx && sha256sum src-tauri/resources/silero_vad_v4.onnx`
Expected: `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28`.

- [ ] **Step 2: Write a unit test** in `src-tauri/src/models_mgr.rs`: `whisper_model_path_points_at_bin` asserts that the path for `whisper-large-v3-turbo-q5` ends with `whisper-large-v3-turbo-q5/ggml-large-v3-turbo-q5_0.bin`, and that the Parakeet path equals the model dir. Implement `fn engine_path(models_root: &Path, m: &ModelInfo) -> PathBuf`.

- [ ] **Step 3: Scaffold the UI** with `npm create vite@latest ui -- --template svelte-ts`. Configure Vite multi-page input (`index.html`, `overlay.html`), and add `@tauri-apps/api` and `vitest`. Run `cd ui && npm run build` and expect `dist/index.html` and `dist/overlay.html`.

- [ ] **Step 4: Implement the shell and commands** as listed above.

- [ ] **Step 5: Add CI job `app`**

- Matrix `macos-14` and `windows-latest`.
- Steps:
  - `npm ci && npm test` in `ui/`
  - Install the Vulkan SDK on Windows (`humbletim/install-vulkan-sdk@v1.2`)
  - `cargo test -p dictation-app`
  - `tauri-apps/tauri-action@v0` to build bundles, uploaded as artifacts (`.dmg`, `-setup.exe`)

Run: push the branch and watch with `gh run watch`.
Expected: both matrix jobs pass, and the artifacts appear on the run page.

- [ ] **Step 6: Commit**

`git commit -am "feat(app): Tauri shell, state, model manager, commands, CI build"`

---

### Task 11: Platform I/O: recorder, hotkey listener, paste, permissions

**Files:**
- Create: `src-tauri/src/platform/{mod.rs, recorder.rs, hotkey_listener.rs, keymap.rs, paste.rs, focus.rs, permissions.rs}`
- Modify: `src-tauri/src/lib.rs` (replace the stub ports)

**Interfaces:**
- Consumes: `Recorder`, `Paster` (Task 9), `CaptureBuffer`, `rms` (Task 5), `HotkeyTracker`, `Event` (Task 4).
- Produces:
  - **`CpalRecorder::new(device: Option<String>, always_on: bool, levels: Box<dyn Fn(f32) + Send>, events: Sender<Event>)`** implements `Recorder`.
    - The callback downmixes to mono and resamples to 16 kHz with `rubato` (`FftFixedIn`), then pushes into `CaptureBuffer::new(200, 600_000)`.
    - It sends `levels(rms)` roughly every 33 ms while recording, and sends `Event::MaxDuration` on `MaxReached`.
    - `stop()` sleeps 150 ms, then calls `take()`.
    - Without `always_on`, the stream opens in `start()` and closes in `stop()`/`cancel()`.
    - A stream error callback sets an error flag, so `stop()` returns `Err("Microphone disconnected")`.
  - **`keymap::code(k: rdev::Key) -> Option<&'static str>`** maps to `KeyboardEvent.code` names. It covers modifiers left/right, letters, digits, F1–F12, Space, Escape, Tab, CapsLock and arrows. On macOS, right Option is `rdev::Key::AltGr` → `"AltRight"`.
  - **`hotkey_listener::spawn(spec: Arc<RwLock<HotkeySpec>>, tx: Sender<Event>)`** runs `rdev::listen` on its own thread, feeds a `HotkeyTracker` with `now_ms`, and forwards events. When the spec changes, it calls `set_spec`.
  - **`SystemPaster`** implements `Paster` (sketched in code after this list):
    - It saves the previous clipboard text (`arboard::get_text`, `None` if it isn't text), sets the new text, and sends Cmd+V (macOS) or Ctrl+V with `enigo`.
    - After 300 ms on a background thread: if the clipboard still equals our text and the previous value was `Some`, it restores the previous value.
    - On Windows, if the foreground window's process is elevated and this process isn't (`OpenProcessToken` + `TokenElevation`), it skips the keystroke and returns `CopiedOnly`. Any enigo error also returns `CopiedOnly`.
  - **`focus::frontmost_app() -> Option<String>`**:
    - macOS: `NSWorkspace.sharedWorkspace.frontmostApplication.localizedName` (`objc2-app-kit`).
    - Windows: `GetForegroundWindow` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW`, then the file stem.
  - **`permissions`**:
    - macOS: microphone status via `AVCaptureDevice authorizationStatusForMediaType:` (`objc2-av-foundation`), accessibility via `AXIsProcessTrusted`.
    - `open_permission_settings` opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone` or `...?Privacy_Accessibility`.
    - Windows: both statuses return `true`.

```rust
// SystemPaster::paste
let prev = clipboard.get_text().ok();
clipboard.set_text(text)?;           // on error → return CopiedOnly
if target_is_elevated() || send_paste_chord().is_err() { return CopiedOnly; }
spawn(move || { sleep(300ms); if clipboard.get_text() == Ok(text) { if let Some(p) = prev { clipboard.set_text(p) } } });
Pasted
```

- [ ] **Step 1: Write failing tests** in `src-tauri/src/platform/keymap.rs` (`#[cfg(test)]`):
  - `keymap_modifiers`: `code(Key::ControlRight) == Some("ControlRight")`, `code(Key::ShiftLeft) == Some("ShiftLeft")`, `code(Key::Escape) == Some("Escape")`, `code(Key::KeyD) == Some("KeyD")`, `code(Key::Space) == Some("Space")`.
  - `keymap_altgr_is_alt_right`: `code(Key::AltGr) == Some("AltRight")`.

- [ ] **Step 2: Implement `keymap.rs`**, then the recorder, listener, paster, focus and permissions. Wire them into `lib.rs`:
  - `CpalRecorder` with `levels` that emits the `audio-level` event to the overlay window.
  - `hotkey_listener::spawn`.
  - `SystemPaster`.

- [ ] **Step 3: Run the tests in CI**

Run: push the branch, then `gh run watch`.
Expected: the `app` job passes on both OSes, including `keymap_*`.

- [ ] **Step 4: Commit**

`git commit -am "feat(app): cpal recorder, global hotkey, paste with clipboard restore, permissions"`

---

### Task 12: Overlay window and overlay port

**Files:**
- Create: `ui/src/overlay/bars.ts`, `ui/src/overlay/bars.test.ts`, `ui/src/overlay/Overlay.svelte`, `ui/src/overlay/main.ts`
- Create: `src-tauri/src/overlay.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `OverlayState` (Task 9; serialized as `{kind, message?}`) and the `audio-level` event (number 0..1).
- Produces:
  - `export function step(current: number[], target: number, dt_ms: number, seed: number): number[]` returns 9 bar heights in [0.15, 1]:
    - Each bar moves toward `target × weight[i]`, with `weight = [0.5,0.7,0.85,1,0.9,1,0.85,0.7,0.5]` plus jitter of ±0.1 from a deterministic hash of `seed + i`.
    - Easing factor `1 - exp(-dt_ms / 60)`.
  - `export function level(rms: number): number` returns `min(1, sqrt(rms) * 3)`, a perceptual boost.
  - **`TauriOverlay`** (Rust, in `overlay.rs`) implements `Overlay`:
    - `Recording`: position the window per `overlay_position` on the monitor with the cursor (24 px margin from the edge), call `show()` without focusing, then emit `overlay-state`.
    - `Done`: hide after 700 ms.
    - `NoSpeech` / `Copied` / `MicError` / `NoModel` / `Error`: hide after 1800 ms.
    - `Hidden`: hide immediately.
    - A newer state cancels a pending hide (generation counter).
    - At setup, call `set_ignore_cursor_events(true)`. On Windows, also set `WS_EX_NOACTIVATE`. On macOS, convert the overlay into a non-activating `NSPanel` with `tauri-nspanel` (style mask `NonactivatingPanel`, level floating, joins all spaces) so it never takes focus.

`Overlay.svelte`:
- A 120 × 36 rounded pill, `background: color-mix(...)`, styled for both themes with `prefers-color-scheme`.
- **Recording:** a red dot plus a `<canvas>` with 9 bars, drawn in a `requestAnimationFrame` loop that runs only while `kind === "recording"`.
- **Processing:** 3 dots pulsing in CSS.
- **Done:** ✓.
- **Messages:** short text: "No speech", "Copied — press ⌘V" (or "Ctrl+V" on Windows), "Mic error", "No model", "Error".
- CSS opacity transitions: 120 ms in, 150 ms out.
- When `prefers-reduced-motion: reduce`, show one dot whose opacity follows `level` instead of the bars.

- [ ] **Step 1: Write failing tests** in `ui/src/overlay/bars.test.ts`

```ts
import { step, level } from "./bars";
test("returns 9 bars within bounds", () => {
  const b = step(Array(9).fill(0.15), 1, 16, 1);
  expect(b).toHaveLength(9); b.forEach(v => { expect(v).toBeGreaterThanOrEqual(0.15); expect(v).toBeLessThanOrEqual(1); });
});
test("silence settles to minimum", () => {
  let b = Array(9).fill(1); for (let i = 0; i < 100; i++) b = step(b, 0, 16, i);
  b.forEach(v => expect(v).toBeCloseTo(0.15, 2));
});
test("loud input raises middle bar more than edges", () => {
  let b = Array(9).fill(0.15); for (let i = 0; i < 100; i++) b = step(b, 1, 16, 7);
  expect(b[3]).toBeGreaterThan(b[0]);
});
test("deterministic for same seed", () => { expect(step(Array(9).fill(.5), .5, 16, 3)).toEqual(step(Array(9).fill(.5), .5, 16, 3)); });
test("level boosts and caps", () => { expect(level(0)).toBe(0); expect(level(1)).toBe(1); expect(level(0.01)).toBeCloseTo(0.3, 5); });
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd ui && npx vitest run src/overlay`
Expected: fail, because the module isn't found.

- [ ] **Step 3: Implement `bars.ts`, `Overlay.svelte`, `main.ts` and `overlay.rs`.** Replace the stub overlay port in `lib.rs`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd ui && npx vitest run src/overlay`
Expected: 5 passed.

- [ ] **Step 5: Commit**

`git commit -am "feat: overlay pill with level-driven bars"`

---

### Task 13: Main window: history, settings, models, first run

**Files:**
- Create: `ui/src/main/api.ts`, `ui/src/main/App.svelte`, `ui/src/main/History.svelte`, `ui/src/main/Settings.svelte`, `ui/src/main/Models.svelte`, `ui/src/main/FirstRun.svelte`, `ui/src/main/HotkeyInput.svelte`
- Create: `ui/src/main/format.ts`, `ui/src/main/format.test.ts`

**Interfaces:**
- Consumes: the Task 10 commands and events, through typed wrappers in `api.ts`. Each command gets one function with the same name in camelCase, e.g. `listHistory(beforeId?: string, limit = 50): Promise<Dictation[]>`. TS types mirror the Rust structs (`Settings`, `Dictation`, `ModelStatus`, `Status`).
- Produces, in `format.ts`:
  - `groupByDay(items: Dictation[], now: Date): { label: string; items: Dictation[] }[]`. Labels are "Today", "Yesterday", else `toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })`. Order is preserved.
  - `formatBytes(n: number): string`: `"0 B"`, `"1.5 KB"`, `"1.9 MB"`, `"2.0 GB"` (1024 base, 1 decimal above B).
  - `formatDuration(ms: number): string`: `"0:08"`, `"1:05"`, `"12:00"`.
  - `hotkeyLabel(keys: string[], mac: boolean): string`: `["AltRight"]`/mac → `"Right ⌥"`, `["ControlRight"]`/win → `"Right Ctrl"`, `["ControlLeft","Space"]`/win → `"Ctrl + Space"`.

Screens:
- **App:** a left nav with History, Settings and Models. Shows FirstRun instead when there's no installed model, or on macOS when a permission is missing.
- **History:**
  - A search box with 250 ms debounce, then `searchHistory`. Otherwise a paginated `listHistory` with "Load more".
  - Day groups. Each row shows the time, the text (2-line clamp), the duration and the target app.
  - Actions: play (`<audio>` with `convertFileSrc(await audioFile(id))`, hidden when there's no audio), copy (`navigator.clipboard.writeText`), re-transcribe, and delete with an inline confirm.
  - Export buttons open the save dialog, then call `exportHistory`.
- **Settings:**
  - `HotkeyInput`: click "Record", then press keys. It collects `KeyboardEvent.code` until all keys are released and rejects a lone `Escape`.
  - Mic select from `listInputDevices`, plus the `mic_always_on` toggle with its note "Instant start; the system mic indicator stays on."
  - Cleanup: toggles, plus editable lists for fillers, replacements (two columns) and vocabulary.
  - Overlay position, launch at login, audio retention, unload after idle, and storage used (`formatBytes(storageUsage())`).
  - Saves with `setSettings` on change, debounced 300 ms.
- **Models:**
  - Shows each catalog entry with its size, installed/active badges, a Download button with a progress bar fed by `model-progress`, Cancel, "Import file…" (file dialog per required file name), and "Use this model".
- **FirstRun:**
  - Step 1 (macOS): Microphone and Accessibility rows with status and "Open Settings". The status re-checks every 2 s.
  - Step 2: download Parakeet (~660 MB), with progress.
  - Step 3: "Hold Right ⌥ (or Right Ctrl) and speak", with a test textarea.

- [ ] **Step 1: Write failing tests** in `format.test.ts`, covering every example listed under `format.ts` above, plus:
  - `groupByDay` with items at now, now−1 day and now−3 days → 3 groups labelled "Today", "Yesterday" and a date.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd ui && npx vitest run src/main`
Expected: fail, because the module isn't found.

- [ ] **Step 3: Implement `format.ts`**, then `api.ts` and the screens.

- [ ] **Step 4: Run the tests and build**

Run: `cd ui && npx vitest run && npm run build`
Expected: all tests pass and the build succeeds.

- [ ] **Step 5: Commit**

`git commit -am "feat(ui): history, settings, models, first-run screens"`

---

### Task 14: Release packaging and manual verification

**Files:**
- Create: `.github/workflows/release.yml`, `docs/manual-test-checklist.md`, `README.md`

**Interfaces:**
- Consumes: the Task 10 build matrix.

- [ ] **Step 1: Add `release.yml`.** On a tag `v*`, it runs `tauri-action` for `macos-14` (target `aarch64-apple-darwin`) and `windows-latest`, and creates a draft GitHub release with the `.dmg` and the NSIS `-setup.exe`. Signing secrets are optional: if `APPLE_CERTIFICATE` is absent, the build is unsigned, and the README explains right-click → Open.

- [ ] **Step 2: Write `docs/manual-test-checklist.md`** with one checkbox per item, for each OS:
  - First-run permissions (macOS) and model download, including Cancel and resume.
  - Hold the hotkey and dictate into a browser text field, Slack, VS Code and Word/Notes. The text appears and the clipboard is restored.
  - The overlay appears within a moment, never takes focus, and lets clicks pass through.
  - Tap under 300 ms does nothing. Esc cancels. Silence shows "No speech". "um, uh" pastes nothing.
  - A 3-minute dictation pastes the full text.
  - Pasting into an elevated window on Windows shows "Copied — press Ctrl+V".
  - Unplug the mic mid-recording: "Mic error", and the next dictation works.
  - History: search, play, copy, re-transcribe, delete and export.
  - Retention changes remove old audio.
  - Switch to the Whisper model and dictate; switch back.
  - Benchmark: a 10 s utterance pastes in about 1 s or less.
  - Idle CPU is about 0% in Activity Monitor / Task Manager.

- [ ] **Step 3: Write `README.md`**: what the app does, install steps, permissions, where data lives, the model download size, and how to run the core tests and the accuracy test.

- [ ] **Step 4: Verify the release workflow**

Run: `git tag v0.1.0-rc1 && git push --tags`, then `gh run watch`.
Expected: a draft release with 2 installers.

- [ ] **Step 5: Run the manual checklist** on a Mac and a Windows PC. The user does this. File any failures as bugs before tagging `v0.1.0`.

- [ ] **Step 6: Commit**

`git commit -am "chore: release workflow, manual checklist, README"`
