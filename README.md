# Open Local Dictation

A tiny tray app that does only dictation: hold a hotkey, speak, and the text is typed into the
application you were using. Speech recognition is Whisper, run on your PC; nothing leaves it.

* **Models:** Whisper tiny, base or small (int8). Download them in the app (SHA-256 verified) or point the
  app at a folder that already holds your models, so other apps can share them. The app never deletes models.
* **Languages:** automatic detection or about 80 Whisper languages, searchable, switchable while dictating.
* **CPU or GPU:** your choice. GPU needs an NVIDIA card and the CUDA libraries (about 1.7 GB, downloaded once, or
  linked from Open Local Assistant if it already has them).
* **Review before inserting (default):** the result opens as editable text. `Ctrl+Enter` inserts,
  `Ctrl+Shift+Enter` retries, `Esc` cancels.
* **Target app:** the overlay shows which application the text will go into and gives focus back to it.

Default hotkey: `Ctrl+Alt+Space` (hold to talk). Everything is configurable in Settings (tray icon → Settings).

## Develop

```text
npm install
npm run tauri dev
cd src-tauri && cargo fmt --all && cargo clippy --all-targets && cargo test
npm run typecheck && npm run build
```

## Build

`build.bat` produces the release exe, a portable folder and (with Inno Setup) a setup file in `release\v<version>\`.
Data lives in `%APPDATA%\com.localdictation.app`.

## Documentation

Everything is described in [`specs/`](./specs): architecture, full behaviour, IPC API, data models, module
catalog, relationships, developer guide and diagrams. `AGENTS.md` explains the rule that every change updates them.

License: GPL-3.0-only.
