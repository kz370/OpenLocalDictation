# Architecture Overview — Open Local Dictation v0.1.0

Hold a hotkey, speak, and the text appears in the application you were using. A small, dictation-only
Tauri 2 desktop app (Windows first). Speech recognition is Whisper (tiny, base, small, large-v3 turbo; one model for all languages or one per language) run locally with
sherpa-onnx. Nothing leaves the machine except the one-time model and CUDA-library downloads.

## 1. Goals and non-goals

| Goal | Notes |
|---|---|
| Very small app | Rust backend + React UI; release exe about 6 MB plus four speech-engine DLLs (about 23 MB). |
| Whisper tiny / base / small | Downloaded on first run, SHA-256 verified. Existing Whisper folders in a shared models folder are used in place. |
| Language selector | Automatic or about 80 Whisper languages; changeable from the overlay while dictating. |
| CPU or GPU | User choice. GPU needs an NVIDIA card and the CUDA libraries (linked from Open Local Assistant when present, else downloaded). |
| Edit before insert | Review is on by default: the result opens as editable text in the overlay. |
| Target-app detection | The overlay shows which application will receive the text and gives focus back to it before typing. |

Non-goals: chat, any LLM, text-to-speech, MCP, history database, text correction, deleting models.

## 2. Layers

```text
React UI (WebView2)
  pages/Overlay.tsx   pages/Settings.tsx   components/*   settingsStore.ts   ipc.ts
        │  invoke(commands) / listen(events)
Tauri backend (Rust, src-tauri/src)
  commands.rs  ── thin IPC layer
  lib.rs       ── dictation flow: voice events → run_dictation → insert / review
  shortcuts.rs ── global hotkey, Esc watcher, review keys
  window.rs    ── overlay + settings windows, target-app capture
  stt/         ── SttService (Whisper), VoiceSessions (VAD + worker), engine (sherpa-onnx)
  audio/       ── cpal microphone capture, 16 kHz mono, level + spectrum
  typing.rs    ── LiveTyper, scancode typing, paste
  models.rs    ── catalog, ModelStore, downloads, custom-model detection
  gpu.rs       ── CUDA pack: relaunch-from-pack-folder, reuse, crash guard
  settings.rs  ── settings.json
  mic.rs       -- microphone present / muted / blocked check (drives the red tray icon and taskbar button)
  taskbar.rs   -- optional mic button inside the taskbar (own thread, native child window)
  tray.rs  autostart.rs  logging.rs  hardware.rs  errors.rs  state.rs
        │
Native: sherpa-onnx + ONNX Runtime DLLs (shared), WASAPI via cpal, Win32 SendInput / window APIs
```

## 3. Windows

| Label | Purpose | Frame |
|---|---|---|
| `overlay` | The dictation card. Transparent, always on top, no taskbar entry, clipped to the card with a window region; does not take focus while listening, takes it in review. | none |
| `settings` | Five tabs (Speech model, Processing, Dictation, Overlay, App). Fixed 780 x 656, not resizable or maximizable. | none; the page draws its own title bar (minimize, close-to-tray) |

A third surface, not a Tauri window, is the **taskbar button** (`taskbar.rs`, opt-in `taskbarWidget`): a small layered native window that is a child of `Shell_TrayWnd`, on its own thread so a stall can never block Explorer. It is the bubble style's circle moved into the taskbar: while it is on, the app draws no circle of its own and the pill (the overlay window, anchored by `window::anchored_geometry`) opens above it on hover, click or dictation. Only a bottom taskbar is supported.

The app lives in the tray. Closing settings only hides them; `Quit` in the tray exits.

## 4. Runtime flow (summary)

1. Hotkey press (`shortcuts.rs`) → `start_dictation`: remember the focused window, start a voice session, show the overlay.
2. `VoiceSessions` captures the microphone, loads the model on a helper thread meanwhile (levels keep moving), feeds the VAD, and transcribes each finished utterance on a worker thread. `partial` events update the overlay.
3. Hotkey release (hold) or second press (toggle), or the overlay's Insert/Review button, stops the session; the final `transcript` event arrives.
4. `run_dictation`: with review on, the text is held in `dictation_review` and the overlay becomes an editor; otherwise it is typed/pasted and the overlay confirms and hides.
5. In review, Ctrl+Enter inserts (focus is first returned to the target window), Ctrl+Shift+Enter retries, Esc cancels.

See `diagrams/sequence.mmd` and `diagrams/dataflow.mmd`.

## 5. GPU model

The ONNX Runtime CUDA build cannot be loaded into a running process because Windows resolves imported DLLs from the exe's folder first. When the pack is installed and `device` is `gpu`, the exe copies itself into `<data>\gpu\cuda-1.13.8\` (next to the CUDA DLLs) and relaunches from there with `LOCAL_DICTATION_GPU_ACTIVE=1`. A start that dies before a model loads leaves a `starting` file; the next start finds it, switches `device` to `cpu` and stays on the CPU. Switching device needs a restart (`app_restart` starts the installed exe with `--delay 1500`).

## 6. Security and privacy

* Audio is processed in memory and never written to disk. Logs contain events and timings, never dictated text.
* Downloads: model files are pinned to Hugging Face URLs with per-file SHA-256; a mismatch deletes the staging folder and reports an error.
* Webview CSP: `default-src 'self'`; scripts only from the app; connect only to IPC.
* Text insertion uses real key events (scancodes) or the clipboard (restored afterwards).
