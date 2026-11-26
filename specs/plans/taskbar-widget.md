# Plan: taskbar widget

Status: proposed, not started. Nothing in this file is implemented yet.

## 1. Goal

An optional mic button that lives inside the Windows taskbar, next to the
other taskbar widgets (the way tools such as TrafficMonitor place their
readouts). It is the bubble style's circle, moved into the taskbar:

- Click starts dictation; click again stops it (same as the overlay mic and
  the tray entry, `shortcuts::toggle_dictation`).
- Hover shows the pill above the taskbar, popping out of the widget.
- Right-click opens a small menu.
- While dictating, the pill shows on its own with the live text and, in
  review mode, the editor.

Off by default. Turned on in Settings > Overlay.

## 2. Non-goals (first version)

- No widget on secondary monitors' taskbars (primary taskbar only).
- No vertical taskbars (left/right edge): the widget is hidden there and the
  setting explains why. Windows 11 does not support them natively anyway;
  third-party taskbar mods do.
- No drawing inside the taskbar's own XAML content. The widget is our own
  window placed over a free area of the taskbar.
- No change to how dictation itself works.

## 3. How it works

### 3.1 Two windows, two jobs

| Part | What it is | Why |
| --- | --- | --- |
| Widget | A small native Win32 window (not a webview), made a child of the taskbar window | It must sit inside the taskbar's z-order, so the taskbar never covers it. A webview parented into another process is fragile and heavy. |
| Pill | The existing overlay window (`window.rs`, label `overlay`), top-level and always on top | Already has the pill, language menu, live text and review editor. It only needs a new anchor point. |

### 3.2 The widget window

- Class registered by us, created with `WS_CHILD | WS_CLIPSIBLINGS`, no
  caption, and made a child of the taskbar with `SetParent`:
  - primary taskbar: `FindWindowW("Shell_TrayWnd", None)`.
- Drawn with GDI double buffering (or Direct2D later): a round button in
  the app's teal (`--accent`), the mic glyph, and the same tinted halo as
  the bubble. The theme (light or dark taskbar) is read from
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme`
  and re-read on `WM_SETTINGCHANGE`.
- States, driven from Rust (no webview involved):
  - idle: teal mic with halo;
  - listening: red stop square with a pulsing ring (a 60 fps timer only
    while listening);
  - transcribing / correcting / reviewing: small spinner;
  - inserted: green check for the usual linger time (`LINGER_OK_MS`);
  - empty / cancelled / error: grey or red cross for `LINGER_MSG_MS`.
- Per-monitor DPI aware: sizes come from `GetDpiForWindow` on the taskbar
  window; the button is 32 logical px inside the taskbar's height.

### 3.3 Its own thread

Making a window a child of a window in another process (Explorer) joins
the two threads' input queues. If our thread then stalls, the whole
taskbar stalls with it. So the widget gets a dedicated thread with its own
message loop, doing nothing but drawing and forwarding input:

- Inputs go out as messages on a channel (`std::sync::mpsc` or a tokio
  channel) to the app: `Clicked`, `HoverStart`, `HoverEnd`, `ContextMenu`.
- State changes come in via `PostMessageW(hwnd, WM_APP + n, ...)` so the
  widget thread never blocks on app locks.
- Nothing slow (model loading, transcription, file I/O) ever runs there.

### 3.4 Where in the taskbar

Windows 11's taskbar is one XAML surface; there is no supported API that
reserves room in it, and the task buttons cannot be shrunk the way the
Windows 10 rebar could. Tools that do this place their window over a part
of the taskbar that is normally empty. Plan:

1. Read the taskbar rectangle (`SHAppBarMessage(ABM_GETTASKBARPOS)`) and
   edge. Only the bottom and top edges are supported.
2. Read the alignment: `HKCU\...\Explorer\Advanced\TaskbarAl`
   (0 = left, 1 = centered; missing means centered on Windows 11).
3. Find the notification area: child `TrayNotifyWnd` of `Shell_TrayWnd`.
   Its left edge is where the tray icons start.
4. Default spot:
   - icons centered: just left of the notification area (the space between
     the task buttons and the tray is normally empty);
   - icons left-aligned: same spot; the space is even larger.
5. A user offset (setting, logical px, dragged with Shift + drag on the
   widget) moves it left from there, so it can sit beside other widgets
   (for example TrafficMonitor) instead of on top of them.
6. Re-place on: `WM_DISPLAYCHANGE`, `WM_DPICHANGED`, `WM_SETTINGCHANGE`
   (taskbar settings), the taskbar's own size changing (poll its rect every
   2 s, it is cheap), and the notification area's width changing (new tray
   icons).

On Windows 10 the same placement works (the tray is also `TrayNotifyWnd`);
the widget then sits over the task band's right end. Shrinking the task
band (`MSTaskSwWClass`) like on Windows 10-era tools is out of scope.

Verify all class names and the placement logic on the current Windows 11
build before relying on them: they are undocumented and change between
releases. Reading the TrafficMonitor source for its taskbar window is a
good cross-check.

### 3.5 Explorer restarts and taskbar changes

- Register `RegisterWindowMessageW("TaskbarCreated")` in the widget thread.
  When it arrives (Explorer restarted), the old parent is gone: destroy the
  widget and create it again under the new `Shell_TrayWnd`.
- If `Shell_TrayWnd` is not found (Explorer not running, or a replacement
  shell), retry every 5 s; after that, log one warning and show the reason
  in Settings next to the switch.
- Auto-hide taskbar (`ABM_GETSTATE` & `ABS_AUTOHIDE`): the widget is a
  child, so it hides and shows with the taskbar for free. The pill anchors
  to the taskbar's shown position.
- Full-screen apps: the taskbar (and so the widget) is hidden by Windows; the
  pill still shows while dictating, as today.

### 3.6 Interaction details

| Input | Result |
| --- | --- |
| Click | `toggle_dictation`. While a result waits for review, the click focuses the editor instead of starting a new take. |
| Hover 300 ms | Show the pill above the widget (idle card). Leaving both widget and pill for 600 ms hides it again (same timings as the bubble, but the open delay is longer because the taskbar is passed over constantly). |
| Right-click | Native popup menu (`TrackPopupMenu`): Start dictation, Language (submenu with the shown languages, check on the current one), Settings, Hide taskbar button. A native menu suits the taskbar and needs no webview room. |
| Shift + drag | Moves the widget along the taskbar; saved as the offset on release. |
| Double-click | Nothing (avoids starting and stopping in one go). |
| Keyboard | The widget does not take focus. The global shortcut keeps working as before. |

Tooltips: the widget shows the shortcut and state via a standard tooltip
control (`TOOLTIPS_CLASS`), since the overlay's themed tooltips live in the
webview.

### 3.7 The pill above the widget

- New anchor mode for the overlay: `OverlayAnchor::Taskbar { x, top }`,
  where `x` is the widget's horizontal center and `top` is the taskbar's top
  edge (physical px).
- `window.rs` places the small card so its center is at `x` and its bottom
  is a few px above `top`, clamped into the work area (the pill is wider
  than the widget, so near a screen edge it shifts sideways).
- The pill pops out from the widget: the page gets the widget's position
  relative to the window (new event `overlay://anchor`) and uses it as the
  `transform-origin` of `.overlay-pill`.
- With the widget on, the floating bubble circle is not shown: the widget
  is the circle. The overlay style setting (pill or bubble) then only
  affects the floating overlay when the widget is off or unavailable.
- Dragging the pill is disabled while it is anchored to the widget (the
  widget's offset is the way to move it).

## 4. Settings and API

New fields in `Settings` (`settings.rs`, `types.ts`, `data_models.txt`):

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `taskbarWidget` | bool | false | Show the mic button in the taskbar. |
| `taskbarWidgetOffset` | i32 | 0 | Logical px moved left from the default spot. Clamped to the taskbar width. |

Settings UI (`Settings.tsx`, Overlay tab):

- Switch "Mic button in the taskbar", hint: "Click to dictate, point at it
  to open the pill. Shift-drag it to move it along the taskbar."
- Under it, when it could not be placed, a one-line reason from the backend
  (for example "The taskbar is on the side of the screen; the button only
  fits a bottom or top taskbar.").
- "Reset position" link that sets the offset to 0.

Commands and events (`commands.rs`, `api_reference.md`):

| Name | Kind | Purpose |
| --- | --- | --- |
| `taskbar_widget_status` | command | `{ shown: bool, reason: string? }` for the Settings line. |
| `taskbar://status` | event | Same payload, sent when it changes (Explorer restart, taskbar moved). |
| `overlay://anchor` | event | `{ x, y }` of the widget center in the overlay window, for the pop-out origin. |

Turning the switch on or off goes through the normal `settings_save`; the
command handler starts or stops the widget when `taskbar_widget` changes
(next to the existing `refresh_overlay_layout` call).

## 5. Code layout

| File | Change |
| --- | --- |
| `src-tauri/src/taskbar.rs` (new, `#[cfg(windows)]`) | Widget thread, window class, drawing, placement, Explorer restart handling, `start(app)`, `stop()`, `set_state(state)`. A no-op stub module on other targets. |
| `src-tauri/src/lib.rs` | `mod taskbar;`, start it at setup when the setting is on; forward widget inputs to `shortcuts::toggle_dictation`, `window::show_idle_overlay_at`, the popup menu actions. |
| `src-tauri/src/window.rs` | `OverlayAnchor`, `show_idle_overlay_at(anchor)`, placement above the taskbar, emit `overlay://anchor`. |
| `src-tauri/src/lib.rs` / `shortcuts.rs` | Wherever `dictation://state` is emitted, also call `taskbar::set_state` (one helper `emit_state(app, payload)` to keep them in step). |
| `src-tauri/src/settings.rs` | The two fields, defaults, sanitize (offset clamp). |
| `src-tauri/src/commands.rs` | `taskbar_widget_status`; start/stop on settings change. |
| `src-tauri/Cargo.toml` | `windows` features: add `Win32_UI_Shell` (app bar messages), `Win32_UI_HiDpi` (DPI), `Win32_UI_Controls` (tooltip), `Win32_System_Registry` (theme, alignment). Already present: `Win32_UI_WindowsAndMessaging`, `Win32_Graphics_Gdi`, `Win32_Foundation`. |
| `src/types.ts`, `src/pages/Settings.tsx`, `src/strings.ts` | Setting fields, switch, status line, error texts. |
| `src/pages/Overlay.tsx`, `src/styles/app.css` | Listen to `overlay://anchor`; hide the bubble circle when anchored; set the pop-out origin. |
| `specs/*` | Per AGENTS.md: `full_documentation.md`, `catalog.txt`, `relationships.txt`, `data_models.txt`, `api_reference.md`, `architecture_overview.md` (new layer), `developer_guide.md` (Cargo features), `diagrams/architecture.mmd`, `runtime.md` log. |

## 6. Phases

Each phase ends with the quality gates from AGENTS.md
(`cargo fmt`, `cargo clippy --all-targets`, `cargo test`,
`npm run typecheck && npm run build`) and a manual check on Windows 11.

1. **Spike (throwaway branch).** A bare widget: find `Shell_TrayWnd`,
   create the child window on its own thread, draw a teal circle left of the
   tray, log clicks. Confirms placement, input and that the taskbar never
   stalls. Decide here whether the placement in 3.4 holds on the current
   Windows 11 build.
2. **Widget module.** `taskbar.rs` with drawing for all states, DPI, theme,
   tooltip, Explorer restart, retry and status. Setting and switch in
   Settings. Click toggles dictation.
3. **Pill anchoring.** `OverlayAnchor`, hover to show the pill above the
   widget, pop-out origin, bubble circle hidden while anchored.
4. **Menu and moving.** Right-click menu (language, settings, hide),
   Shift + drag offset, reset.
5. **Hardening and docs.** Auto-hide, multi-monitor (primary only), light
   and dark taskbar, 100 / 125 / 150 / 200 % scaling, specs and README.

## 7. Test checklist (manual, Windows 11)

- Widget appears left of the tray; does not cover task buttons or tray
  icons with centered and with left-aligned icons.
- Click starts and stops dictation; text lands in the app that had focus
  (the widget never takes focus).
- Hover shows the pill above the widget; leaving hides it; the language menu
  opens inside the screen.
- Review mode: the editor opens above the pill; Insert, Retry, Cancel work.
- An empty take shows "nothing heard" and no editor.
- Explorer restart (`taskkill /f /im explorer.exe`, then start it): the
  widget comes back within a few seconds.
- Taskbar auto-hide on: the widget hides and shows with it.
- Light and dark Windows theme; switch while running.
- Scaling 100 %, 125 %, 150 %, 200 %; moving the app between monitors with
  different scaling.
- Taskbar moved to the top (with a taskbar tool) and to the side: top
  works, side shows the reason in Settings.
- Next to TrafficMonitor: the offset moves the widget beside it.
- Turning the switch off removes the widget at once; on brings it back.
- CPU while idle stays at 0 % (no timers running unless listening).

## 8. Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Windows 11 updates change the taskbar's window structure | Widget misplaced or missing | Placement isolated in one function; status line in Settings; feature stays opt-in. |
| Shared input queue with Explorer | Taskbar freezes if our thread blocks | Dedicated widget thread that never waits on app state (3.3). |
| Covers other widgets or new taskbar features | Overlap | User offset; default spot hugs the tray. |
| Anti-cheat or security tools flag cross-process `SetParent` | Rare warnings | Opt-in; documented in the hint. |
| Taskbar replacements (ExplorerPatcher, StartAllBack) | Different class layout | Status reason "taskbar not recognised"; no crash. |

## 9. Open questions

1. Should hover show the pill at all, or only click-and-hold? Hover is
   proposed, with a 300 ms delay so passing over the taskbar does nothing.
2. Right-click menu: native (proposed, fits the taskbar) or the pill's own
   language menu?
3. When the widget is on, should the global shortcut's overlay also anchor
   above the widget (proposed: yes, one consistent place), or keep its
   dragged position?
4. Secondary monitors: is "primary taskbar only" fine for the first version?
