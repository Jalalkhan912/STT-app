# Dictation

Offline push-to-talk dictation for macOS (Apple Silicon) and Windows. Hold a hotkey, speak, release: the transcript is pasted into the focused app. Speech recognition runs locally (Parakeet by default, Whisper optional); no audio leaves your machine. Includes a history of dictations, text cleanup rules and a small on-screen overlay.

Default hotkey: Right Option (macOS), Right Ctrl (Windows). Esc while holding cancels.

## Install

Download the `.dmg` (macOS) or `-setup.exe` (Windows) from the draft release on the GitHub Releases page, or from the `bundles-*` artifacts of a CI run (Actions tab).

On first launch, the app asks you to download a model: Parakeet TDT 0.6B v2 (int8), about 660 MB. Whisper models are optional and can be added from the Models tab.

### macOS: opening an unsigned app
Builds are not notarized (ad-hoc signed at best), so Gatekeeper blocks the first launch. On macOS 15 and later, right-click then Open no longer bypasses this. Either:
- Try to open the app, then go to System Settings → Privacy & Security and click "Open Anyway"; or
- Run `xattr -dr com.apple.quarantine /Applications/Dictation.app`.

### Permissions
macOS needs three permissions (the first-run screen walks you through them):
- Microphone: to record.
- Accessibility: to paste into the focused app.
- Input Monitoring: for the global hotkey.

With ad-hoc signing, macOS may ask for Accessibility and Input Monitoring again after each update. Remove the old Dictation entry in System Settings → Privacy & Security and add it again.

Windows needs no special permissions. Pasting into an elevated (administrator) window is not possible; the text is copied and you press Ctrl+V.

## Where data lives

`<OS app data>/Dictation/`, i.e. `~/Library/Application Support/Dictation/` on macOS and `%APPDATA%\Dictation\` on Windows:
`models/`, `recordings/`, `history.db`, `settings.json`, `logs/`.

## Tests

```
cargo test -p dictation-core
cd ui && npm test
```

Accuracy test (needs the Parakeet model files under `$DICTATION_MODELS/parakeet-tdt-0.6b-v2-int8`; CI runs it on macOS):

```
DICTATION_MODELS=~/models cargo test -p dictation-core --features parakeet --test accuracy -- --ignored --nocapture
```

Manual checks: [docs/manual-test-checklist.md](docs/manual-test-checklist.md).

## Releases

Pushing a tag `v*` runs `.github/workflows/release.yml`, which builds the installers and creates a draft release. Optional secrets for signing: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`. Without `APPLE_CERTIFICATE` the app is ad-hoc signed.
