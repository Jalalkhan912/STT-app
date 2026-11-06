# Manual test checklist

Run on a build from the draft release or a CI artifact. macOS is the primary platform; Windows items follow.
Default hotkey: Right Option (macOS), Right Ctrl (Windows).

## macOS (primary)

### First run and permissions
There are 3 permissions: Microphone, Accessibility (paste), Input Monitoring (global hotkey).
- [ ] Fresh state: the window opens to the First-run screen.
- [ ] Microphone "Allow..." shows the system prompt.
- [ ] Accessibility "Allow..." and Input Monitoring "Allow..." open the system prompts or the right Settings pane; "Open Settings" opens the right pane.
- [ ] Each status flips to Granted within about 2 s of granting.
- [ ] If the hotkey does nothing after granting Input Monitoring, restarting the app fixes it.
- [ ] Hotkey hint shows "Right ⌥"; the test textarea receives pasted text.
- [ ] With Accessibility missing, dictation does not paste and the overlay shows "Copied — press ⌘V"; Cmd+V then pastes the text.

### Model download
- [ ] Parakeet download: the progress bar moves.
- [ ] Cancel stops the download; starting again resumes (does not restart from 0).
- [ ] A failure (e.g. network off) shows an error; Continue enables only on completion.
- [ ] Models tab: Import file... dialog, "Use this model" and badges work.
- [ ] Banner "Active model not installed" appears when the active model is missing and links to Models.

### Dictation
- [ ] Hold the hotkey and dictate into: a browser text field, Slack, VS Code, Notes/Word. Text appears (Cmd+V paste works) in each.
- [ ] The previous clipboard text is restored about 300 ms later; a non-text clipboard (image) is left holding the transcript.
- [ ] A tap under 300 ms does nothing.
- [ ] Esc while holding cancels.
- [ ] Silence shows "No speech".
- [ ] "um, uh" pastes nothing.
- [ ] A 3-minute dictation pastes the full text.
- [ ] A 10 s utterance pastes in about 1 s or less.
- [ ] Unplug or change the mic mid-recording: "Mic error", and the next dictation works.
- [ ] A recording reaching 10 minutes stops at the maximum duration.
- [ ] With "unload model when idle" set, the first press after unloading reloads the model without hanging the keyboard.

### Overlay
- [ ] The pill appears within a moment of pressing the hotkey.
- [ ] The app you are typing in keeps focus (caret keeps blinking, paste goes there); no Dock or menu-bar change.
- [ ] Clicks pass through the pill to the window beneath.
- [ ] Appears over a full-screen app and on every Space.
- [ ] Bars react to your voice; fade-in and fade-out look right; reduced-motion mode shows a single dot.
- [ ] Text fits in 120x36: "Copied — press ⌘V", "No speech", "Mic error", "No model", "Error".
- [ ] Each of the 4 positions applies immediately after changing the setting, 24 px from the edge and above the Dock for bottom positions.
- [ ] Mixed-DPI: with a 1x external display next to the Retina screen, hold the hotkey with the cursor on each display; the pill appears on the display under the cursor, fully on-screen.
- [ ] No leftover pill after Done or an error.
- [ ] No CPU/GPU use while the pill is hidden (Activity Monitor).

### History
- [ ] Search (debounced) finds entries; "Load more" works past 50 items.
- [ ] Play audio works; a missing file shows "Can't play audio".
- [ ] Copy, Re-transcribe and Delete (with confirm) work.
- [ ] Export JSON, CSV and audio save dialogs write files; "Export saved." shows.
- [ ] Changing retention removes old audio.

### Settings
- [ ] Hotkey input: record a combination including ⌘ combos; it commits when keys are released or 400 ms after the last key-up. Escape alone is rejected/cancels.
- [ ] Changes persist after relaunch.
- [ ] Launch at login toggles the login item.
- [ ] Non-default input device works; "mic always on" (restart-only) gives pre-roll after restart.
- [ ] Other restart-only settings take effect after restart.
- [ ] Storage figure is shown.
- [ ] Switch to a Whisper model and dictate; switch back to Parakeet.
- [ ] Light and dark appearance both look right.
- [ ] Ad-hoc signed build: after updating, Accessibility and Input Monitoring may need to be removed and re-added.

### Idle
- [ ] Idle CPU is about 0% in Activity Monitor.

## Windows

- [ ] Model download with Cancel and resume works.
- [ ] Hold Right Ctrl and dictate into a browser field, Slack, VS Code and Word; text appears and the clipboard is restored.
- [ ] Pill appears, does not take focus, is click-through, and is not in the taskbar or Alt-Tab.
- [ ] Tap under 300 ms does nothing; Esc cancels; silence shows "No speech"; "um, uh" pastes nothing.
- [ ] A 3-minute dictation pastes the full text.
- [ ] Pasting into an elevated window shows "Copied — press Ctrl+V".
- [ ] Unplug the mic mid-recording: "Mic error", and the next dictation works.
- [ ] History: search, play, copy, re-transcribe, delete, export; retention removes old audio.
- [ ] Hotkey recording, launch at login and restart-only settings work.
- [ ] Switch to Whisper (Vulkan build) and dictate; switch back.
- [ ] A 10 s utterance pastes in about 1 s or less.
- [ ] Idle CPU is about 0% in Task Manager.
