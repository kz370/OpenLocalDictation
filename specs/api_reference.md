# API Reference — Tauri IPC (Open Local Dictation v0.1.0)

Frontend wrappers live in `src/ipc.ts` (`ipc.*` for commands, `on()` for events). Every command that
can fail returns `Result<T, AppError>`; errors reach the UI as `{ code, detail }` (see §4).
There are **25 commands**.

## 1. Commands

### Settings and app
| Command | Args | Returns | Notes |
|---|---|---|---|
| `get_settings` | — | `Settings` | |
| `save_settings` | `settings: Settings` | `Settings` | Sanitizes and saves. Keeps `overlayX/Y`, `overlayPinned`, `overlayMini`, `overlayMiniWidth` and `modelsDir` (owned by dragging, the overlay commands and `models_set_dir`). Unloads the recognizer when `model` or `device` changed, re-registers the hotkey when `shortcut` changed, updates the autostart entry when `startWithOs` changed. Emits `settings://changed` and `gpu://changed`. |
| `shortcut_errors` | — | `string[]` | Registration failures from the last `register_all`. |
| `shortcuts_capture` | `capturing: bool`, `reviewShortcuts?: bool` | `()` | `true` releases all global shortcuts (settings is recording a new one) or, with `reviewShortcuts`, swaps in the review keys; `false` re-registers the normal ones. |
| `app_info` | — | `AppInfo` | version, paths, NVIDIA info, current and default models folder. |
| `open_logs` | — | `()` | Opens the logs folder in Explorer. |
| `audio_inputs` | — | `AudioDevice[]` | Real microphones only; loopback devices (Stereo Mix, virtual cables) are excluded. |
| `settings_minimize` | — | `()` | Title-bar button of the frameless settings window. |
| `settings_close` | — | `()` | Re-registers shortcuts and hides the settings window (app keeps running in the tray). |
| `app_restart` | — | `()` | Starts the installed exe with `--delay 1500` and quits this process (GPU/CPU switch). |

### Speech models
| Command | Args | Returns | Notes |
|---|---|---|---|
| `models_list` | — | `ModelEntry[]` | The three catalog models, then every Whisper folder found in the models folder (`custom: true`, id `custom:<path>`). |
| `models_download` | `ids: string[]` | `()` | Queues catalog models; progress on `models://download`. Only called after the user pressed Download. |
| `models_cancel` | `id: string` | `()` | |
| `models_set_dir` | `dir?: string` | `string` | Existing absolute folder, or empty for the app's own folder. Refused during a download or recording. Unloads the recognizer, resets `model`, saves `modelsDir`. Nothing is moved or copied. Returns the folder now in use. |

There is **no** command that deletes a model.

### GPU / CPU
| Command | Args | Returns | Notes |
|---|---|---|---|
| `gpu_status` | — | `GpuStatus` | |
| `gpu_set_enabled` | `enabled: bool` | `()` | Sets `device` to `gpu`/`cpu`, unloads the recognizer. Effective after a restart. |
| `gpu_install` | — | `()` | NVIDIA only. If Open Local Assistant's pack exists it is linked (hard link, copy fallback) and the call returns when done; otherwise the pack downloads in the background with progress on `gpu://download`. On success `device` becomes `gpu`. |
| `gpu_cancel` | — | `()` | Cancels a running pack download. |

There is **no** command that removes the CUDA libraries.

### Dictation overlay
| Command | Args | Returns | Notes |
|---|---|---|---|
| `dictation_cancel` | — | `()` | Esc / X: discards review, drops recorded audio, sets the cancel flag, hides the overlay. |
| `dictation_insert_now` | `text: string` | `()` | Stops listening. With review on, opens the editor first (`dictation://state` `reviewing`). |
| `dictation_confirm` | `text: string` | `()` | Inserts the edited text: shrinks the overlay, restores focus to the target window, waits 150 ms, types/pastes. |
| `dictation_retry` | — | `()` | Throws the take away (retracting live-typed text) and listens again. |
| `dictation_set_language` | `language: string` | `()` | Saves `language`; emits `settings://changed`. |
| `dictation_reset_overlay_position` | — | `()` | Clears `overlayX/Y` and moves the overlay to its default spot. |
| `overlay_set_pinned` | `pinned: bool` | `()` | Also the idle overlay's X button (unpin = close the window, never quits the app). Saves `overlayPinned`, emits `settings://changed`. Pinned: the overlay shows now and stays on screen (always on top), falling back to an idle card after each dictation (event `overlay://idle`). Unpinned and idle: hides. |
| `overlay_set_mini` | `mini: bool` | `()` | Saves `overlayMini`, emits `settings://changed`, re-applies the small (400x68) or full (480x176) layout. Review stays small: its editor floats above the pill. |
| `dictation_toggle` | — | `()` | Overlay mic button: starts dictation, or stops and inserts when listening. |
| `overlay_resize` | `width: u32, commit: bool` | `()` | Small overlay width in logical px (clamped 320-800). Applies to the window live; `commit` saves `overlayMiniWidth` and emits `settings://changed`. Ignored while `overlaySizeLocked`. |
| `dictation_overlay_tip` | `tip: [x,y,w,h]?` | `()` | Adds a tooltip's rectangle to the overlay window's visible region (kept apart from the menu/bubble rectangle), or removes it. |
| `dictation_overlay_menu` | `menu?: [x,y,w,h]` | `()` | Adds the open language menu to the overlay window region (or removes it). |

## 2. Events (backend → UI)

| Event | Payload | Target | Meaning |
|---|---|---|---|
| `dictation://state` | `DictationStateEvent` | all | `listening`, `transcribing`, `idle`, `reviewing` (editor open, still transcribing), `review` (result ready), `inserted`, `empty`, `cancelled`, `error`. |
| `dictation://target` | `{ targetApp }` | all | Friendly name of the window text will go into. |
| `dictation://review-shortcut` | `"insert" \| "retry" \| "cancel"` | overlay | Review keys pressed while another app has focus. |
| `voice://event` | `VoiceEvent` | overlay | Only `level` (value + 24 spectrum bands) and `partial` (live text). |
| `settings://changed` | `Settings` | all | |
| `models://changed` | — | all | Re-read `models_list`. |
| `models://download` | `DownloadProgress` | all | |
| `gpu://changed` | — | all | Re-read `gpu_status`. |
| `gpu://download` | `GpuProgress` | all | |

## 3. Types
See `data_models.txt` for every payload shape.

## 4. Error codes
`stt_unavailable`, `audio`, `download`, `invalid`, `io`, `cancelled`, `other`. The UI maps each to a
message saying what happened and how to fix it (`src/strings.ts`, `ERRORS`); `detail` is shown under
"Technical details".

## 5. Command-line arguments
| Arg | Meaning |
|---|---|
| `--delay <ms>` | Sleep before starting (max 10 s), so a restart waits for the old process to release the single-instance lock. |
| `--minimized` | Passed by the Windows autostart entry; the app has no main window so it changes nothing. |
