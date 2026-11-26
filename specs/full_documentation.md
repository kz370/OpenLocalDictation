# Full Documentation — Open Local Dictation v0.1.0

Index: [architecture_overview.md](./architecture_overview.md) · [api_reference.md](./api_reference.md) ·
[catalog.txt](./catalog.txt) · [relationships.txt](./relationships.txt) · [data_models.txt](./data_models.txt) ·
[developer_guide.md](./developer_guide.md) · [runtime.md](./runtime.md) · [diagrams/](./diagrams)

## 1. Summary

Open Local Dictation is a tray app that types what you say into whatever application has keyboard focus.
It was split out of Open Local Assistant's dictation feature and keeps that feature's behaviour, but has no
chat, no LLM, no text-to-speech, no correction profiles and no database.

## 2. User-facing behaviour

### 2.1 First run
No model is installed, so the Settings window opens by itself on the **Speech model** tab. The user either
downloads Whisper tiny, base or small (one at a time, with progress, cancel, and SHA-256 verification) or
points the app at a folder that already holds models (§2.6). Until a model exists, pressing the hotkey shows
an overlay error: "No speech model is ready. Open Settings and download Whisper tiny, base or small."

### 2.2 Dictating
* Default hotkey `Ctrl+Alt+Space`, **hold to talk** (alternatively press to start / press to stop). Modifier-only combinations need at least two modifiers (a single Alt would fire on every Alt press).
* On press the app remembers the focused window, shows the overlay (never taking focus) and starts listening. The header shows the state, a live 24-band spectrum and the language pill; a tab above the card reads "Insert into: <app>" and follows focus changes to other applications while dictating.
* Speech is split into utterances by a voice-activity detector; each finished utterance is transcribed and appended, so text grows while the user still talks. A cold model load takes seconds; the meter keeps moving and audio heard meanwhile is kept and processed first.
* `Esc` cancels from anywhere (polled, never swallowed). The overlay's X, **Start over** and **Insert/Review** buttons do the same from the mouse. A take also ends after `autoStopSilenceSecs` of silence (default 8 s) or 5 minutes.

### 2.3 Review before insert (default on)
After the take, the overlay turns into an editable text area ("Review — edit if needed"), becomes focusable, and global review keys are registered so they work even though the destination app owns focus:
`Ctrl+Enter` insert · `Ctrl+Shift+Enter` retry · `Ctrl+Shift+Backspace` cancel (and `Esc`). **Copy** puts the text on the clipboard. On insert the overlay shrinks back, focus is returned to the remembered window (`AttachThreadInput` + `SetForegroundWindow`), 150 ms later the text is inserted.
With review off, words are typed live as they are committed; "Insert" at the end reconciles the screen to the final text.

### 2.4 How text is inserted
* **Typing** (default): real key events by scancode using the destination window's keyboard layout, 16 ms per character. This is byte-exact in Notepad and Chromium; the Unicode `SendInput` path drops and duplicates characters in classic edit controls and is used only for characters the layout cannot produce (emoji, or Arabic under an English layout).
* **Pasting**: the clipboard is set, Ctrl+V is sent, the previous clipboard text is restored 300 ms later.
* Live typing is **word-committed**: only whole words reach the screen (`committed_prefix`), so a word still being revised never needs backspacing. If an injection fails the typer marks itself desynced and the next apply erases what was confirmed and retypes.
* Before any one-shot insert the app waits (max 3 s) for the hotkey's modifier keys to be released, so Ctrl/Alt do not turn typed characters into shortcuts.
* "Add a space after the text" appends one space to the final text.

### 2.5 Language
The overlay's language pill (and Settings → Dictation → Language) selects Automatic or one of ~80 Whisper languages with a filter box. The choice is saved. Mid-take, changing it restarts the take. Whisper switches language on a loaded model in place (`stt::engine::set_language`, well under a millisecond) instead of reloading. Settings → Dictation → "Languages in the overlay menu" switches the overlay menu between All and a chosen set (dialog with checkboxes, checked languages grouped on top with a divider above the rest; `shownLanguages`, empty = all). The current language and Automatic are always listed, and Settings → Language lists the same chosen set.

### 2.6 Models folder
`modelsDir` points at a folder shared with other apps; nothing is moved or copied.
* Catalog models are found in `<dir>/stt/<id>` or `<dir>/<id>` (so choosing the `stt` folder itself works) when `.complete` exists **or** the folder holds valid Whisper files.
* Any other folder directly in `<dir>` or `<dir>/stt` with `encoder*.onnx`, `decoder*.onnx`, `*tokens.txt` and no file containing `joiner` is listed as **Existing** (e.g. `whisper-turbo`).
* Downloads go to `<dir>/stt/<id>` (or `<dir>/<id>` when `<dir>` is named `stt`).
* **Use default folder** asks for confirmation. Changing the folder is refused during a download or recording, unloads the recognizer and resets `model` to automatic.
* The app **never deletes models** (no command, no button).

### 2.7 Processing (CPU / GPU)
* Segmented control CPU / GPU (NVIDIA). GPU is disabled when DXGI finds no NVIDIA adapter.
* Choosing GPU without the CUDA libraries shows a notice: **Use existing** (link Open Local Assistant's pack, instant) or **Download** (≈ 1.7 GB, progress, cancel).
* A device change shows "takes effect after a restart" with **Restart now**.
* The app never removes the CUDA libraries.
* Crash guard: if a GPU start dies before a model loads, the next launch switches `device` to `cpu`.

### 2.8 Settings window
Frameless, fixed 780 x 656, no resize or maximize (when a tab is taller than the window only the rows of its card scroll; the title, tabs and card heading stay fixed); own title bar (app icon, minimize, close-to-tray). The title bar and the header brand mark show the real app icon (`public/app-icon.png`, a copy of `src-tauri/icons/128x128.png`; replace it whenever the app icon changes). Tabs: **Speech model** (folder + models), **Processing**, **Dictation** (shortcut, behaviour, language, microphone, review, insert method, trailing space, overlay position), **App** (start with Windows, theme, logs). The page never scrolls, and the model list is shown in full (the window height covers it). Every Reset button has an icon and asks first through the themed `Dialog`.

### 2.9 Tray
Left click opens the idle dictation window. Menu: Show dictation window (opens the idle overlay; nothing is recorded until the shortcut or the mic button), Settings, Quit. A second launch opens settings.

## 3. Technical details

### 3.1 Speech pipeline (`stt/`)
1. `VoiceSessions::start` validates a model exists, stops any earlier session (joining its thread), opens the microphone (`Capture::start`: cpal, mono downmix, resample to 16 kHz, 100 Hz high-pass, retry up to 4 times if the driver kills the stream within 250 ms) and spawns the session thread.
2. `warm_up` loads the recognizer on a scoped helper thread while the session thread forwards levels and buffers samples.
3. `run`: samples feed a Silero VAD (CPU, window 512, `vadThreshold`, min silence `silenceMs`, min speech 250 ms, max 20 s). Each finished segment (>= 350 ms) is sent to a worker thread that runs `Recognizer::transcribe` and `clean_transcript`; results become `partial` events. Audio not covered by a segment stays in `buffer` and is transcribed once more at the end.
4. On stop the remaining queue and buffer are transcribed; one `transcript` event carries the final text. With `discard` (cancel/retry) nothing is emitted.
5. `clean_transcript` removes bracketed captions (`[BLANK_AUDIO]`, `(applause)`, `<|nospeech|>`), music notes, and known silence hallucinations ("Thank you.", "Untertitel der Amara.org-Community", …).

### 3.2 Overlay window
480 x 176 card inside a window with 300 px of invisible room above and below (for the language menu). `SetWindowRgn` clips the window to the card, the destination-app tab and the open menu, so the transparent room neither shows nor blocks clicks. Frame styles are stripped before every region change (Windows otherwise draws a classic border). Listening uses `WS_EX_NOACTIVATE`; review clears it. All transitions run on the main thread (`on_main`). Dragging uses manual `setPosition` with Shift = horizontal-only and Alt = vertical-only; the card position is saved (`overlayX/Y`) except for programmatic moves (`suppress_persistence`).

### 3.3 Target application
`remember_target` stores the foreground HWND at start (ignoring this app's own windows). `watch_dictation_target` polls every 100 ms while a take is active and republishes `dictation://target` when focus moves to another external window. The friendly name comes from the process image name (`application_display_name` maps Code, Notepad, Word, Excel, PowerPoint, Chrome, Edge, Firefox, else the exe stem).

### 3.4 Downloads
`ModelStore::install` streams each file to `.staging-<id>`, hashing as it goes (progress every 250 ms), compares SHA-256 after each file, and only then renames the folder into place and writes `.complete`. Cancel, error or mismatch removes the staging folder.

### 3.5 Errors
Rust errors serialize as `{code, detail}`. The UI shows the mapped fix text; `detail` sits under "Technical details" in `ErrorNotice`.

### 3.6 Logging and privacy
Daily rolling log in the Tauri log dir, filter via `LOCAL_DICTATION_LOG` (default `info`). Dictated text and audio are never logged.

## 4. Business rules
* One take at a time (`dictation_busy`, session numbers); a new take cannot start during review or transcription.
* Cancel always wins over a pending transcript (`dictation_cancel` consumed by `run_dictation`).
* Review and live typing are mutually exclusive: with review on, `partial` events type nothing.
* A late `idle` of an old session never ends a newer one in the UI (session numbering, overlay ignores `idle` while previewing/reviewing).
* The overlay never hides while a take is active or busy (`overlay_hide_is_current_and_idle`).

## 5. Dependencies
Tauri 2 (tray, global-shortcut, single-instance, dialog) · sherpa-onnx 1.13.8 (+ -sys, shared) · cpal · enigo + Win32 `SendInput` · arboard · reqwest (native-tls) · sha2 · tar/bzip2/zip · auto-launch · windows crate · React 19 · zustand · lucide-react · Vite 8.

## 6. Relationship to Open Local Assistant
Independent repository, data folder (`%APPDATA%\com.localdictation.app`) and identifier (`com.localdictation.app`). The only contact: the CUDA pack of Open Local Assistant (`%APPDATA%\com.localassistant.app\gpu\cuda-1.13.8`) is linked when present, and models may live in a folder both apps point at.


### Pinned and small overlay

`overlayPinned` keeps the overlay window shown and always on top. `hide_overlay_window` (window.rs) then docks it as an idle card instead of hiding it and emits `overlay://idle`, which the page uses to reset itself. Nothing is recorded while idle: only the global shortcut, the tray entry or the overlay's mic button (`dictation_toggle`) start `voice.start()`. `overlayMini` switches the window to a 400 x 68 pill (mic, short level meter, destination app name, expand, pin, cancel, in that order; a subtitle bubble above the pill shows the live text, only while there is some, and is added to the window region through `dictation_overlay_menu`; no destination-app tab, no language menu). In review the same bubble becomes the editor (textarea, a hint row with the review keys, a Copy | Retry | Cancel button group, Insert), so the pill never grows; `set_overlay_review` keeps the bubble's region rectangle in the small layout. While listening the mic button becomes a stop (insert) button and the full card has a Cancel button (the header X cancels too). The Overlay tab of Settings has the pin, small, lock-size and position options; the overlay header has pin and size buttons. `overlayHorizontalOnly` (on by default) limits dragging to the x axis: the page zeroes the vertical movement and `show_overlay_now` puts the card back on the bottom edge of its monitor's work area (`docked_y`), whatever y was saved. `overlayPositionLocked` disables dragging (and the double-click reset). A gear button in the header (both layouts) opens Settings through `settings_open`; `overlayShowSettings` (Settings > Overlay) hides it. `overlayOpacity` (30-100 %, Settings > Overlay) sets the CSS variable `--ov-a`, which mixes the overlay card and subtitle bubble backgrounds with transparent; text and buttons stay solid. `overlayLayouts` ("small" by default, "large" or "both") limits which sizes exist: the size button only appears with "both", and `Settings::effective_mini` decides the layout (small -> always mini, large -> never, both -> `overlayMini`).

### Shortcut while busy

Pressing the dictation shortcut while the last take is still transcribing (`VoiceSessions::is_transcribing`) drops that take (`cancel_transcription`: its `Transcript` is never emitted) and starts a new recording; while a result waits for review it discards the review and starts again. The new recording begins once the running transcription pass ends (it cannot be interrupted).

### Resizable small overlay and tooltips

The small pill has a drag handle on its right edge (width only, 320-800 px, double-click resets to 400). `overlay_resize` resizes the window live and saves the width on release; `overlaySizeLocked` (Settings, Overlay tab) removes the handle. Tooltips are themed: `src/components/Tooltip.tsx` (`TooltipLayer`, mounted once per window in `main.tsx`) replaces the native `title` tooltip by moving the text to `data-tip` while hovered and drawing a `.tooltip` using the theme tokens. In the overlay window no bubble is drawn (the card sits on the taskbar and the window is clipped to it): `TooltipLayer inline` hands the text to the overlay, which shows it in its header in place of the status label while the pointer is on a button.

### Settings icons and duplicate-text fix

Tabs, card headings, setting rows (`Row icon`), segmented options and Cancel/Save/Edit/Dismiss buttons carry lucide icons (`Card icon`, `Segmented option icon`). In `stt/session.rs` the audio of a finished utterance is now cut from the tail buffer by its stream position (`SpeechSegment::start` + length, tracked against `buffer_base`), not by a length guess: with a long pause before speaking, part of the utterance used to stay in the buffer and was transcribed a second time when recording stopped, so the text appeared twice.

### Language menu in the small pill

The small pill has a language chip (globe + code) and the same menu opens on a right click anywhere on the pill header (`PillMenu` `openRef`). The menu's rectangle (`dictation_overlay_menu`) and the bubble's (`dictation_overlay_bubble`) are separate window-region rectangles; the bubble is hidden while the menu is open. Picking a language while listening restarts the take in that language, like in the full card.

Small overlay style (`overlayStyle`, Settings > Overlay, shown unless the layout is large only): `pill` (default) is one pill with the mic inside, always shown. `bubble` keeps only a 44 px circle on screen (`.overlay-card.mini.bubble`, centered in the pill's box) holding the state icon and the mic button; the pill, without the mic, shows 6 px above it while dictating, while hovered (after 120 ms; it hides 600 ms after the pointer leaves) and while its language menu is open. A right click on the circle shows the pill. The pill pops out of the circle (`.overlay-pill` fades in while scaling up from the circle's center with a slight overshoot, and shrinks back into it when hidden; no transform at rest). Pill and circle are separate shapes with a 6 px gap; the mic button has a tinted halo. The live text and the review editor float above the pill. The window never moves or resizes for this: the page sends the visible shapes with `dictation_overlay_card` (circle, plus the pill while shown; the pill's shape is added before it shows and dropped once it has gone back into the circle) and the window region is cut to them (`CARD_RECTS` in `window.rs`, small layout only). `MENU_ROOM` is 300 px so the editor fits above the pill.

The small pill always fits its header: every header item keeps its natural width and only the status label shrinks (to 44 px, with an ellipsis). The page measures the header (`pillMinWidth`) on each render and calls `overlay_fit_width` when the window is narrower than that; the command only grows the width (clamped 320-800), saves it, and applies even while `overlaySizeLocked`. The resize handle cannot drag below the measured width.

The language menu uses concentric corners: menu radius 16 px with 5 px padding, rows 11 px (`MENU_RADIUS` in `window.rs` matches), an even shadow so the region clip cuts every corner the same, a fixed-width code column and the check mark at the row's end.
