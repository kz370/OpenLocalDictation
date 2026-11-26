# Developer Guide — Open Local Dictation v0.1.0

## 1. Prerequisites
* Windows 10/11 x64, WebView2 runtime.
* Rust (MSVC toolchain) and Node.js 20+.
* Optional: Inno Setup 6/7 for the installer, an NVIDIA GPU to test GPU mode.

## 2. Layout
```text
src/                 React UI (overlay + settings), Vite, TypeScript strict
src-tauri/           Rust backend (crate local-dictation, lib local_dictation_lib)
specs/               source of truth (this folder)
scripts/build.bat     release build + portable folder and zip + installer + release notes
scripts/bump-version.bat (+ .ps1)  bumps the version everywhere and writes the release notes
website/             project website (GitHub Pages), deployed by .github/workflows/pages.yml when a v* tag is pushed (or run by hand)
local-dictation.iss  Inno Setup script
AGENTS.md            contributor rules
```

Windows manifest: `src-tauri/build.rs` passes tauri-build a custom app manifest (common controls v6, as before, plus the Windows 10/11 `supportedOS` entry). Without that entry Windows treats the exe as pre-Windows 8, where a layered window cannot be a child window, and the taskbar button (`taskbar.rs`) would not be created. The `windows` crate features `Win32_System_LibraryLoader`, `Win32_System_Registry` and `Win32_UI_HiDpi` serve that module; `Win32_Media_Audio`, `Win32_Media_Audio_Endpoints` and `Win32_System_Com` serve `mic.rs`.

## 3. Run
```text
npm install
npm run tauri dev        # Vite on 127.0.0.1:1430 + debug Rust build
```
The first Rust build downloads the sherpa-onnx prebuilt archive (about 40 MB) into `src-tauri/target/sherpa-onnx-prebuilt`.
Set `SHERPA_ONNX_ARCHIVE_DIR` to a folder holding `sherpa-onnx-v1.13.8-win-x64-shared-MT-Release-lib.tar.bz2` (for example `.build-cache\sherpa-onnx`) to avoid the download after a `cargo clean`; `build.bat` does this automatically when that folder exists.

### Common problems
| Symptom | Cause / fix |
|---|---|
| `The process cannot access the file because it is being used by another process (os error 32)` in `sherpa-onnx-sys` | A running `local-dictation.exe` (the app only hides to the tray) holds the DLLs in `target\debug`. Quit it from the tray or `taskkill /IM local-dictation.exe /F`, then rebuild. |
| Port 1430 in use | A previous Vite is still running (`strictPort`). Stop it. |
| Nothing happens on the hotkey | See Settings → Dictation for a shortcut registration warning; another app may own the combination. |

## 4. Quality gates
```text
cd src-tauri && cargo fmt --all && cargo clippy --all-targets && cargo test
npm run typecheck && npm run build
```
`cargo test` runs about 30 unit tests (settings, models, GPU pack helpers, transcript cleaning, typing, audio maths). Two tests are `#[ignore]` (need NVIDIA archives / print timings).

## 5. Release build
`scripts/build.bat` → `release\v<version>\`:
* `Open-Local-Dictation-<version>-portable\` — exe plus `sherpa-onnx-c-api.dll`, `sherpa-onnx-cxx-api.dll`, `onnxruntime.dll`, `onnxruntime_providers_shared.dll`
* `Open-Local-Dictation-<version>-portable.zip` — the portable folder zipped
* `Open-Local-Dictation-<version>-setup.exe` (when Inno Setup is found); `local-dictation.iss` sets `PrivilegesRequired=admin` and installs for all users into `{autopf}` (Program Files)
* `RELEASE_NOTES.md` — copied from `release-notes<version>.md` when that file exists

The version comes from the `version` line of `src-tauri/Cargo.toml`. Do not edit versions by hand: `scripts/bump-version.bat patch|minor|major|X.Y.Z` (add `-DryRun` to preview; the `.bat` just starts `scripts/bump-version.ps1` with the execution policy bypassed) updates `Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `package.json`, `package-lock.json` and the title line of four specs documents, writes `release-notes<version>.md` from the Conventional Commit subjects since the last `v*` tag (tracked in git, unlike `release\`) and logs the bump in `specs/runtime.md`. Passing the current version only rewrites its notes. Release steps: bump, review and commit, `build.bat`, then `gh release create v<version> release<version>\*.exe release<version>\*.zip --title v<version> --notes-file release-notes<version>.md`. Builds use half the CPU threads (`--jobs`).
Release profile: `opt-level = "s"`, LTO, `codegen-units = 1`, `panic = "abort"`, `strip`. Exe about 6.4 MB; the four DLLs about 22.8 MB (`onnxruntime.dll` 17.8 MB).

## 6. Adding things
### A command
1. Write the `#[tauri::command]` in `commands.rs`, register it in `lib.rs` (`generate_handler!`).
2. Add the wrapper in `src/ipc.ts` and any type in `src/types.ts`.
3. Update `specs/api_reference.md` (and the command count), `catalog.txt`, `relationships.txt`.
### A setting
Add the field to `Settings` (+ default + `sanitize`), `src/types.ts`, the UI row, `specs/data_models.txt`. Fields owned by another command (like `modelsDir`, `overlayX/Y`) must be preserved in `save_settings`.
### A model
Add a `CatalogModel` in `models.rs` with the exact file sizes and SHA-256 of each file (`curl https://huggingface.co/api/models/<repo>/tree/main` lists `lfs.oid` for large files; hash small files yourself). Update `data_models.txt`.
### UI rules
Use `Dialog` for every confirmation; every reset/restore button gets an icon and a confirmation; no purple; English only; new dropdowns use `SearchSelect` (menu matches its 340px button and long option text wraps; never horizontal scroll); keep every settings tab inside the fixed 780 x 690 window without page scroll.

## 7. Debugging
* Logs: Settings → App → Open logs (`LOCAL_DICTATION_LOG=debug` for more; the microphone level is logged once per second at debug level).
* The overlay is a normal webview: open it by running the app and attaching WebView2 devtools (F12 works in debug builds) .
* `%APPDATA%\com.localdictation.app\settings.json` can be edited by hand while the app is closed; a damaged file falls back to defaults.

## 8. Specs workflow
Follow `AGENTS.md` §1. Quick checklist per change: `full_documentation.md` (behaviour), `catalog.txt` (new symbols), `relationships.txt` (new edges), `data_models.txt` (types), `api_reference.md` (commands/events), diagrams if flow changes, then one line in `runtime.md` → Log.
