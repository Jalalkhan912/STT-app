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

Release builds signed with the project's self-signed certificate keep these permissions across updates. Ad-hoc signed builds (e.g. CI artifacts) don't: remove the old Dictation entry in System Settings → Privacy & Security, add it again, and restart the app (the setup screen has a Restart button).

### Troubleshooting permissions (macOS)
- Check the microphone entitlement is in the build: `codesign -d --entitlements - /Applications/Dictation.app` should list `com.apple.security.device.audio-input`.
- Reset stale permission entries (for example after installing a new build): `tccutil reset All app.dictation.desktop`, then reopen the app and grant them again.
- If you previously clicked "Don't Allow" for the microphone, macOS won't ask again; enable Dictation under System Settings → Privacy & Security → Microphone.

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

Pushing a tag `v*` runs `.github/workflows/release.yml`: it runs the core tests, builds the installers and creates a draft release.

macOS signing, in order of preference:
1. **Self-signed certificate** (repo secrets `MACOS_SIGNING_P12` = base64 .p12, `MACOS_SIGNING_P12_PASSWORD`, `MACOS_SIGNING_IDENTITY` = the certificate's SHA-1 fingerprint). The identity stays the same across builds, so macOS keeps the Accessibility and Input Monitoring permissions when you update. Gatekeeper still asks for "Open Anyway" on first install.
2. **Apple Developer ID** (secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, optionally `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` for notarization).
3. Neither: ad-hoc signed (permissions must be granted again after each update).
