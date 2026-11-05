# Offline Push-to-Talk Dictation App — Design

Date: 2026-11-05
Status: Draft for review
Working name: **Dictation** (rename anytime; only the app ID and data folder depend on it)

## 1. Goal

A desktop app similar to Wispr Flow: hold a shortcut key, speak, release, and the transcribed text is pasted where the cursor is. It runs fully offline, uses few resources, and transcribes English with high accuracy. Every recording and transcript is kept in a searchable history.

### Requirements (from the user)
- Hold a global shortcut to record; release to transcribe and paste into the focused app.
- A small, efficient animation shows the app is listening.
- All audio recordings and text outputs are saved in the app.
- Works without internet. Efficient. High-quality, accurate speech-to-text.

### Decisions made during brainstorming
- Platforms: **macOS (Apple Silicon)** and **Windows** (with NVIDIA GPU and CPU-only).
- Language: **English only**.
- Text processing: **light, rule-based cleanup** (no LLM).
- Stack: **Tauri 2** (Rust core + Svelte/TypeScript UI).
- Default engine: **NVIDIA Parakeet TDT 0.6B v2 (int8, ONNX Runtime)**; **Whisper via whisper.cpp** optional.

### Success criteria
- Overlay appears ≤ 50 ms after key down; no clipped first or last word.
- ≤ ~1 s from key release to paste for a 10 s utterance on a modern CPU-only laptop; faster on Apple Silicon / NVIDIA.
- WER on the bundled English test clips stays below the threshold set in the test suite (initially 8%).
- Idle: ~0% CPU, app (excluding loaded model) ≤ ~100 MB RAM.
- No network traffic except a model download the user asks for.

### Out of scope for v1
AI rewriting, other languages, toggle/hands-free mode, per-app styles, sync/accounts, Linux.

## 2. Architecture

A single Tauri process. The Rust core owns all logic; the web UI renders the windows.

| Module | Job | Depends on |
|---|---|---|
| `hotkey` | Watch the global shortcut; send `Pressed` / `Released` / `Cancel` (Esc) events | `rdev` (key down/up for modifier-only keys), `global-hotkey` |
| `audio` | Keep a warm input stream; while recording, store 16 kHz mono f32 samples in memory; send RMS levels (~30 Hz) | `cpal`, resampler (`rubato`) |
| `engine` | Trait `Engine { fn transcribe(&self, samples: &[f32]) -> Result<Transcript> }`; implementations `ParakeetEngine`, `WhisperEngine` | `ort` (ONNX Runtime), `whisper-rs` |
| `vad` | Voice activity detection: detects "no speech" and chunks recordings > 2 min | Silero VAD (ONNX) |
| `cleanup` | Pure function `clean(raw, &CleanupConfig) -> String` | none |
| `paste` | Save clipboard → set text → simulate Cmd+V / Ctrl+V → restore clipboard after ~300 ms | `arboard`, `enigo` |
| `history` | Store dictations in SQLite + WAV files; search, delete, export, retention | `rusqlite` (FTS5), `hound` |
| `models` | Catalog, download with progress, SHA-256 check, import a local file, load/unload | `reqwest`, `sha2` |
| `settings` | Load/save/migrate `settings.json` | `serde` |
| `controller` | State machine wiring all modules; the only module that knows the flow | all of the above, via traits |

Every side-effecting module (audio, engine, paste, hotkey, history) is used by the controller **through a trait**, so the controller can be tested with fakes.

### Windows (UI)
1. **Overlay**: transparent, borderless, always-on-top, never takes focus, click-through. Created once at startup; shown/hidden.
2. **Main window**: History, Settings, Models. Opened from the tray / menu bar.
3. **Tray / menu bar icon**: open app, pause hotkey, quit. Launch at login (optional).

### Data folder (`<OS app data>/Dictation/`)
```
models/        parakeet-tdt-0.6b-v2-int8/, silero-vad.onnx, whisper-*.bin
recordings/    YYYY/MM/<ulid>.wav
history.db
settings.json
logs/
```

## 3. Dictation flow

States: `Idle → Recording → Transcribing → Pasting → Idle` (plus `Cancelled`, `Error`).

1. **Key down** → start buffering (the stream is already open, so no startup delay); also keep a 200 ms pre-roll ring buffer so the first word isn't clipped; show the overlay; remember the focused app.
2. **While held** → append samples; send levels to the overlay.
3. **Key up** → record 150 ms more audio (tail), then stop; overlay switches to "processing".
4. **VAD** → if no speech, show "No speech", save the history row with status `no_speech` (no audio file), return to Idle.
5. **Transcribe** → engine runs on a background thread. Recordings over 2 min are split at VAD silences into chunks of ≤ 30 s.
6. **Cleanup** → apply rules.
7. **Paste** → into the focused app; restore the clipboard.
8. **Save** → write the WAV file and the history row in the background; overlay shows ✓ and fades out.

### Edge cases
| Situation | Behavior |
|---|---|
| Hold < 300 ms | Cancel silently, nothing saved |
| Esc while holding | Cancel, discard audio, nothing saved |
| No speech detected | Overlay "No speech"; nothing pasted |
| Recording reaches 10 min | Auto-stop and process as if key released |
| Key pressed while still transcribing | Ignored |
| Paste fails / focus changed / elevated target on Windows | Leave text on the clipboard; overlay "Copied — press ⌘V / Ctrl+V"; status `copied_only` |
| Mic missing or permission denied | Overlay error; click opens Settings or the OS permission page |
| No model installed | First-run setup to download or import one |
| Engine error | Overlay "Error"; audio is still saved with status `error` so it can be re-transcribed |

### Permissions
- macOS: Microphone (`NSMicrophoneUsageDescription`) and Accessibility (global key hook + simulated paste). A first-run screen checks both and links to System Settings.
- Windows: none. Known limit: cannot paste into elevated (admin) apps unless this app also runs elevated → falls back to clipboard.

## 4. Overlay

- Pill of about 120 × 36 px, bottom-center by default (position can be changed in settings); follows the OS light/dark theme.
- **Recording**: red dot + 9 bars on one `<canvas>`, driven by RMS levels (~30 Hz) and eased toward the target values at 60 fps.
- **Processing**: 3 pulsing dots. **Result**: ✓ then fade out, or a short message ("No speech", "Copied — press ⌘V", "Error").
- Fade in 120 ms, out 150 ms. `requestAnimationFrame` runs only while visible → no CPU use while idle.
- If the OS "reduce motion" setting is on, show a single softly pulsing dot instead.

## 5. History and storage

SQLite table `dictations`:

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | ULID (time-sortable) |
| `created_at` | INTEGER | Unix ms |
| `duration_ms` | INTEGER | |
| `text` | TEXT | Final cleaned text (what was pasted) |
| `raw_text` | TEXT | Engine output before cleanup |
| `audio_path` | TEXT NULL | Relative path; NULL if audio deleted/not kept |
| `model` | TEXT | e.g. `parakeet-tdt-0.6b-v2-int8` |
| `target_app` | TEXT NULL | Best effort |
| `status` | TEXT | `pasted` / `copied_only` / `no_speech` / `error` |

FTS5 virtual table on `text` for search, kept in sync with triggers. Schema version stored in `PRAGMA user_version`; migrations run at startup.

- **Audio**: 16 kHz mono 16-bit WAV (~1.9 MB/min).
- **History UI**: newest first, grouped by day; search; for each entry play / copy / re-transcribe / delete (deletes row + file); export CSV/JSON + zip of audio.
- **Retention**: keep audio forever (default) / 30 d / 7 d / never; text kept until deleted. Cleanup runs at startup and every 24 h. Settings shows how much storage is used.
- **Privacy**: everything local; the only network access is a model download the user starts.

## 6. Cleanup rules

`clean(raw, cfg)` applies, in order:
1. Trim; collapse repeated whitespace.
2. Remove filler words (editable list; default `um, uh, er, ah, hmm`), matched as whole words, case-insensitive, then fix commas and spacing left behind.
3. Replacements (user pairs, whole-word, case-insensitive), e.g. `new line` → `\n`, `new paragraph` → `\n\n`.
4. Custom vocabulary: case-insensitive whole-word match → replaced with the canonical spelling (e.g. `kubernetes` → `Kubernetes`).
5. Capitalize the first letter (on by default).
6. Append a trailing space (on by default) so the next dictation doesn't run into this one.

Empty result → treated as no speech.

## 7. Settings (`settings.json`, versioned)

hotkey (default Right Option on macOS / Right Ctrl on Windows), input device, active model, cleanup config (fillers on/off + list, replacements, vocabulary, capitalize, trailing space), overlay position, launch at login, audio retention, unload model after N minutes idle (default: never).

## 8. Models

| ID | Size | Backend | Default |
|---|---|---|---|
| `parakeet-tdt-0.6b-v2-int8` | ~650 MB | ONNX Runtime: CoreML (mac) / DirectML or CPU (win) | Yes |
| `whisper-large-v3-turbo-q5` | ~550 MB | whisper.cpp: Metal / CUDA / CPU | Optional |
| `whisper-distil-large-v3.en-q5` | ~500 MB | whisper.cpp | Optional |
| `silero-vad` | ~2 MB | ONNX Runtime | Shipped in installer |

Catalog = URL + SHA-256 + size. Download resumes after interruption, and the SHA-256 is checked before the model is used. Import from file supports offline machines. The loaded model stays in memory; a warm-up run happens at load time.

## 9. Packaging

- macOS: `.dmg` (Apple Silicon); Info.plist microphone permission string; signing/notarization optional for personal use.
- Windows: NSIS/MSI installer; ONNX Runtime with DirectML + CPU. CUDA whisper.cpp backend is an optional download, so the base installer stays small.
- Models are not bundled (except VAD).
- CI: GitHub Actions builds both platforms and runs tests.

## 10. Testing

- **Unit**: cleanup rules; controller state machine with fake hotkey/audio/engine/paste/history (all edge cases in §3); history DB (CRUD, FTS, retention, migrations); settings load/migrate; model catalog checksum verification.
- **Engine accuracy**: small set of English WAV clips with reference transcripts; assert WER below threshold; print a timing benchmark (real-time factor).
- **Manual checklist per OS**: hotkey in browser/Slack/VS Code/Word; paste + clipboard restore; permissions flow; overlay never takes focus; Esc cancel; elevated-window fallback on Windows; long (> 2 min) dictation.
- The development server has no display/mic; the real app is tested by the user on their Mac/Windows machines, with CI builds as the source of installers.

## 11. Planning amendments

- Parakeet runs on CPU on every platform in v1. ORT GPU providers are deferred and revisited only if the Task 8 benchmark misses the ~1 s target.
- Whisper GPU: Metal on macOS, **Vulkan** on Windows (works on NVIDIA without a CUDA toolkit; replaces the "optional CUDA download").
- The `whisper-distil-large-v3.en` entry is removed: there's no maintained English-only distil GGML build. Turbo q5 remains.
- Microphone: by default the stream opens on key down. Setting `mic_always_on` (default `false`) keeps it warm, which enables the 200 ms pre-roll; the OS mic indicator then stays on. With it off, pre-roll is whatever arrives after the stream opens.
- VAD is Silero v4 (`silero_vad_v4.onnx`, 1 807 522 bytes, sha256 `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28`), bundled as a Tauri resource.
