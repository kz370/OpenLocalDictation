# Runtime — Open Local Dictation

Pipeline state and progress log. Append one line per change under `## Log` (see AGENTS.md §1).

## Files to Process
- 1. Root: `AGENTS.md`, `README.md`, `LICENSE`, `build.bat`, `local-dictation.iss`, `package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`, `.gitignore` (10)
- 2. Backend: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json}` (4); `src-tauri/src/` — `main.rs lib.rs state.rs errors.rs settings.rs models.rs gpu.rs hardware.rs logging.rs autostart.rs tray.rs shortcuts.rs window.rs typing.rs commands.rs` (15), `audio/{mod,capture,devices}.rs` (3), `stt/{mod,engine,session}.rs` (3)
- 3. Frontend: `src/{main.tsx,ipc.ts,types.ts,strings.ts,settingsStore.ts,knownLanguages.ts}` (6), `src/pages/{Overlay,Settings}.tsx` (2), `src/components/{SearchSelect,ShortcutInput,controls,layout,VoiceBars,ErrorBoundary}.tsx` (6), `src/styles/{tokens,base,app}.css` (3)
- 4. Specs: `specs/{architecture_overview.md,full_documentation.md,api_reference.md,developer_guide.md,runtime.md,catalog.txt,relationships.txt,data_models.txt}` (8), `specs/diagrams/{architecture,dataflow,sequence,classes}.mmd` (4)

Files to Process: 65 (assets, icons and lock files are excluded)

## Files Processed
Files Processed: 65 — COMPLETE

## Log
- [2026-11-26T15:00:00Z] Project created as its own repository from the dictation feature of Open Local Assistant, with the full specs set. Behaviour at this point: Whisper tiny/base/small, searchable language lists, CPU/GPU choice with CUDA-pack reuse, review before insert (default on), user-selectable shared models folder with detection of existing Whisper folders, frameless fixed-size tabbed settings window, themed confirmation dialogs for every reset, and no way to delete models or the CUDA libraries.
- [2026-11-26T16:00:00Z] Settings model list no longer scrolls inside its card (removed 200px cap); settings window height 700 -> 780 so the list fits. Specs updated.
- [2026-11-26T16:30:00Z] Added Arabic and German to knownLanguages; overlay language menu can be limited to chosen languages (settings.shownLanguages, empty = all); settings window height 780 -> 750.
- [2026-11-26T16:45:00Z] Language picker dialog groups checked languages on top, divider, then the rest.
- [2026-11-26T17:00:00Z] SearchSelect menu: wider (up to 380px), long options wrap, no horizontal scroll.
- [2026-11-26T17:10:00Z] SearchSelect: menu matches button again (340px wide in rows), no focus outline on the menu; long text still wraps.
- [2026-11-26T17:20:00Z] Settings window height 750 -> 690, measured from the running app (Dictation tab, the tallest, ended ~90px above the bottom).
- [2026-11-26T17:30:00Z] App icon (all src-tauri/icons files) and the settings header brand mark recoloured from teal to green; shape unchanged. Rebuild needed for the exe/tray icon.
