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
- [2026-11-26T18:00:00Z] Settings Language dropdown lists only the chosen languages when a set is chosen (plus Automatic and the current one).
- [2026-11-26T18:30:00Z] Pinned (always-on-top, idle) and small overlay: settings overlayPinned/overlayMini, commands overlay_set_pinned/overlay_set_mini/dictation_toggle, overlay mic/pin/size buttons, Settings switches.
- [2026-11-26T18:50:00Z] Tray 'Dictate' now 'Show dictation window' (idle overlay, no recording); idle overlay X closes the window via overlay_set_pinned(false).
- [2026-11-26T19:10:00Z] Overlay: growing from the small to the full card (review) clamps the card into the work area, so it is no longer cut off at the screen bottom.
- [2026-11-26T19:30:00Z] Overlay UI: stop button (square mic) while listening in both layouts; explicit Cancel button in the full listening row.
- [2026-11-26T19:50:00Z] Shortcut during transcribing or review drops that take and starts a new recording (shortcuts.rs dispatch, VoiceSessions is_transcribing/cancel_transcription).
- [2026-11-26T20:10:00Z] Tray left click opens the idle dictation window instead of Settings (Settings stays in the tray menu).
- [2026-11-26T20:30:00Z] Cancel now returns a pinned overlay to its idle card after the message (hide_overlay_later on both cancelled paths); small overlay widened to 340 and shows the live words while listening.
- [2026-11-26T21:00:00Z] Small overlay redesigned: 340x68 card with controls row and a two-line live-text area (idle hint, live words, results) instead of one truncated line.
- [2026-11-26T21:30:00Z] Small overlay back to a 340x68 pill; live text is a subtitle bubble above it (shown only with text, region via dictation_overlay_menu).
- [2026-11-26T21:45:00Z] Small overlay: level meter stretches over the pill's free width.
- [2026-11-26T22:10:00Z] Small overlay: 400x68 pill with short meter and destination app name; review editor is the bubble above the pill (no more growth to the full card).
- [2026-11-26T22:30:00Z] Small pill: level meter now sits next to the mic button, destination app name after it.
- [2026-11-26T22:50:00Z] Review actions: Copy, Retry and new Cancel grouped as one connected button group beside Insert; small pill's app name centered in the free space.
- [2026-11-26T23:10:00Z] Review editor shows its keys (Ctrl+Enter insert, Ctrl+Shift+Enter retry, Esc cancel) as a hint row and in button tooltips; the keys themselves are unchanged.
- [2026-11-26T23:40:00Z] Small overlay: width-only resize handle (overlay_resize, overlayMiniWidth 320-800, overlaySizeLocked option in Settings). Themed tooltips (components/Tooltip.tsx) replace native title tooltips; overlay adds them to its region via dictation_overlay_tip.
- [2026-11-27T00:00:00Z] Settings: overlay options (pin, small, lock size, position) moved from the Dictation tab to a new Overlay tab; the Dictation tab no longer overflows.
- [2026-11-27T00:20:00Z] Settings window height 690 -> 640 (the Overlay tab split made the Dictation tab shorter).
- [2026-11-27T00:40:00Z] New setting overlayLayouts (small | large | both, default small) replaces the Small overlay switch; the size button shows only with both.
- [2026-11-27T01:00:00Z] New setting overlayOpacity (30-100, default 100) with a slider in Settings > Overlay; applies to the overlay card and bubble background.
- [2026-11-27T01:20:00Z] Overlay header gear button (settings_open) in both layouts; overlayShowSettings option hides it.
- [2026-11-27T01:40:00Z] Settings title bar and header brand mark now show the app icon (public/app-icon.png) instead of a drawn mic glyph.
- [2026-11-27T02:00:00Z] Overlay position options: overlayHorizontalOnly (default on) and overlayPositionLocked, in Settings > Overlay.
- [2026-11-27T02:20:00Z] Settings: the main area scrolls when a tab is taller than the window (Overlay tab grew), so nothing is clipped.
- [2026-11-27T02:40:00Z] Scrollbars: thinner (8px, 4px thumb), themed colours; removed scrollbar-width on the settings area, which had switched the themed ::-webkit-scrollbar rules off.
- [2026-11-27T03:00:00Z] Settings: only the rows of a card scroll (.card-body); title, tabs, banners and the card heading are fixed.
- [2026-11-27T03:20:00Z] Settings card: clipped to its rounded corners and scrollbar track inset, so the scrollbar no longer pokes out at the corner.
- [2026-11-27T03:40:00Z] Settings window height 640 -> 656 and less bottom padding: the Speech model tab scrolled although it fits (a card only scrolls when its rows really overflow).
- [2026-11-27T04:00:00Z] Fix: dictated text appeared twice (tail buffer kept part of an already-transcribed utterance after a long leading pause); finished utterances are now cut off by stream position. Settings: icons on tabs, card headings, rows, segmented options and buttons.
- [2026-11-27T04:30:00Z] Overlay tooltips now show inside the header (TooltipLayer inline) instead of a bubble that fell under the taskbar; dictation_overlay_tip and the tooltip region rectangle removed.
- [2026-11-27T04:50:00Z] The shortcut shown in the Settings intro and the overlay idle hint uses prettyAccelerator (Win, not Super; Press/Hold follows the mode), same as the shortcut field.
- [2026-11-27T05:10:00Z] Overlay: no tooltip on the drag area (the whole header showed a long hint); hint text in the header is plain, not italic.
- [2026-11-27T05:30:00Z] Small pill: language chip and right-click language menu; bubble rectangle split from the menu rectangle (dictation_overlay_bubble).
- [2026-11-27T05:50:00Z] Dropdowns (SearchSelect, overlay language menu) get a search box when they list more than 4 options (was 6 / always).
- [2026-11-27T06:30:00Z] Small pill folds into a centered mic circle while idle and not hovered (dictation_overlay_card, CARD_RECT region); hover unfolds it. Header items keep natural width and the pill grows to fit them (overlay_fit_width, pillMinWidth). Language menu restyled with concentric 16/11 px corners and an even shadow (MENU_RADIUS 16).
- [2026-11-27T07:10:00Z] Replaced the folding pill with an optional bubble style (overlayStyle pill|bubble): mic circle on screen, pill without the mic grows out of it on hover or while dictating, joined by a neck. dictation_overlay_card now takes a list of shapes (CARD_RECTS); MENU_ROOM 240 -> 300.
- [2026-11-27T07:40:00Z] Bubble style: wider neck (88 px, 12 px gap, curves into the circle's outline); bubble pill centers the status with the language chip and buttons on the edges (.overlay-head.split).
- [2026-11-27T08:00:00Z] Bubble neck reshaped (120 x 29 rounded inverted triangle into the circle's sides); pill bottom border and circle ring masked under it instead of covered; neck added to the region shapes.
- [2026-11-27T08:20:00Z] Bubble neck is a straight-sided inverted pyramid (96 x 19); pill and neck now pop out of the circle (fade + scale from its center) instead of morphing from it.
- [2026-11-27T08:40:00Z] Bubble neck replaced by a chin (152 x 51): rounded joints, straight sides tangent to the circle, circle's lower half as its bottom; tinted halo on the mic.
- [2026-11-27T08:50:00Z] Fix: the chin SVG no longer takes clicks (pointer-events: none); its box covered the mic once the pill showed, so the circle's mic did nothing.
- [2026-11-27T09:00:00Z] Bubble chin removed: pill and circle are separate shapes with a 6 px gap (ORB_GAP 6); no masks or neck region shape.
- [2026-11-27T09:20:00Z] Fix: language menu was cut off past the window's left edge when the chip sits on the left (bubble pill); it now aligns to the chip's outer side and stays inside the window.
- [2026-11-27T09:40:00Z] Fix: no edit window when the transcript is empty (live preview skipped without text; an empty final result closes the preview and restores focus).
