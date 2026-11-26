//! Thin IPC layer: validates input, delegates to the modules.

use crate::audio::devices::{self, AudioDevice};
use crate::errors::AppError;
use crate::gpu::{self, GpuStatus};
use crate::models::{self, ModelEntry};
use crate::settings::Settings;
use crate::state::{AppPaths, AppState};
use crate::{autostart, shortcuts, window};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

pub type CmdResult<T> = Result<T, AppError>;

/// Key used for the GPU pack in the shared download table.
const GPU_DOWNLOAD: &str = "gpu-pack";

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Saves the settings and applies what changed: the hotkey, the autostart
/// entry, and the loaded model (dropped when the model or device changed).
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let before = state.settings.get();
    let saved = state.settings.update(|s| {
        // The overlay position is owned by dragging the overlay, not by this form.
        let (x, y, dir) = (s.overlay_x, s.overlay_y, s.models_dir.clone());
        *s = settings;
        s.models_dir = dir;
        s.overlay_x = x;
        s.overlay_y = y;
    })?;
    if saved.model != before.model || saved.device != before.device {
        state.stt.unload();
    }
    if saved.shortcut != before.shortcut {
        shortcuts::register_all(&app);
    }
    if saved.start_with_os != before.start_with_os {
        if let Err(e) = autostart::set_enabled(&app, saved.start_with_os) {
            tracing::warn!(error = %e, "could not change the autostart entry");
        }
    }
    let _ = app.emit("settings://changed", &saved);
    let _ = app.emit("gpu://changed", ());
    Ok(saved)
}

#[tauri::command]
pub fn shortcut_errors(state: State<'_, AppState>) -> Vec<String> {
    state
        .shortcut_errors
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

/// Releases the hotkeys while the settings window records a new one, or swaps
/// in the review-mode keys while the overlay editor is open.
#[tauri::command]
pub fn shortcuts_capture(
    app: AppHandle,
    capturing: bool,
    review_shortcuts: Option<bool>,
) -> CmdResult<()> {
    if capturing {
        if review_shortcuts.unwrap_or(false) {
            shortcuts::register_review_shortcuts(&app).map_err(AppError::Other)?;
        } else {
            shortcuts::unregister_all(&app);
        }
    } else {
        shortcuts::register_all(&app);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub paths: AppPaths,
    pub has_nvidia: bool,
    pub gpu_name: Option<String>,
    /// Folder the models are read from right now.
    pub models_dir: String,
    /// The app's own models folder (what "Use default" goes back to).
    pub default_models_dir: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        paths: state.paths.clone(),
        has_nvidia: state.hardware.has_nvidia(),
        gpu_name: state.hardware.nvidia_name(),
        models_dir: state.models.root().to_string_lossy().to_string(),
        default_models_dir: state
            .paths
            .data_dir
            .join("models")
            .to_string_lossy()
            .to_string(),
    }
}

#[tauri::command]
pub fn open_logs(state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.paths.logs_dir.clone();
    std::fs::create_dir_all(&dir)?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::Other(format!("could not open {}: {e}", dir.display())))
}

#[tauri::command]
pub async fn audio_inputs() -> CmdResult<Vec<AudioDevice>> {
    tokio::task::spawn_blocking(devices::list_inputs)
        .await
        .map_err(|e| AppError::Audio(e.to_string()))
}

// ---------------------------------------------------------------------------
// Speech models
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn models_list(state: State<'_, AppState>) -> Vec<ModelEntry> {
    let downloading = state.downloads.lock().unwrap_or_else(|p| p.into_inner());
    let mut list: Vec<ModelEntry> = models::CATALOG
        .iter()
        .map(|m| ModelEntry {
            id: m.id.to_string(),
            name: m.name.to_string(),
            blurb: m.blurb.to_string(),
            download_bytes: m.download_size(),
            installed: state.models.is_installed(m.id),
            downloading: downloading.contains_key(m.id),
            custom: false,
            path: None,
        })
        .collect();
    // Whisper models already in the folder (put there by the user or another app).
    list.extend(
        state
            .models
            .custom_models()
            .into_iter()
            .map(|c| ModelEntry {
                id: c.id,
                name: c.name,
                blurb: "Found in the models folder".into(),
                download_bytes: 0,
                installed: true,
                downloading: false,
                custom: true,
                path: Some(c.dir.to_string_lossy().to_string()),
            }),
    );
    list
}

/// Points the app at a folder that already holds Whisper models (shared with
/// other apps), or back at its own folder with an empty `dir`. Nothing is
/// moved or copied; new downloads go to the chosen folder.
#[tauri::command]
pub fn models_set_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    dir: Option<String>,
) -> CmdResult<String> {
    let custom = dir.map(|d| d.trim().to_string()).unwrap_or_default();
    let target = if custom.is_empty() {
        state.paths.data_dir.join("models")
    } else {
        let p = std::path::PathBuf::from(&custom);
        if !p.is_absolute() || !p.is_dir() {
            return Err(AppError::Invalid(format!(
                "\"{custom}\" is not an existing folder; pick the folder with the folder picker"
            )));
        }
        p
    };
    if state
        .downloads
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .keys()
        .any(|k| k != GPU_DOWNLOAD)
    {
        return Err(AppError::Invalid(
            "a model download is still running; wait for it to finish or cancel it, then change the folder".into(),
        ));
    }
    if state.voice.is_active() {
        return Err(AppError::Invalid(
            "the microphone is in use; finish dictating, then change the folder".into(),
        ));
    }
    // The loaded model holds its files open; release it before switching.
    state.stt.unload();
    state.models.set_root(target.clone());
    let saved = state.settings.update(|s| {
        s.models_dir = custom.clone();
        // A model chosen in the old folder may not exist in the new one.
        s.model = String::new();
    })?;
    let _ = app.emit("models://changed", ());
    let _ = app.emit("settings://changed", &saved);
    Ok(target.to_string_lossy().to_string())
}

/// Downloads the given models one after another. Called only after the user
/// pressed the download button.
#[tauri::command]
pub async fn models_download(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<()> {
    let list: Vec<&'static models::CatalogModel> =
        ids.iter().filter_map(|id| models::find(id)).collect();
    if list.len() != ids.len() {
        return Err(AppError::Invalid("unknown model id".into()));
    }
    let store = state.models.clone();
    {
        let mut d = state.downloads.lock().unwrap_or_else(|p| p.into_inner());
        for m in &list {
            if !store.is_installed(m.id) {
                d.entry(m.id.to_string()).or_default();
            }
        }
    }
    let _ = app.emit("models://changed", ());
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        for m in list {
            let state = app2.state::<AppState>();
            let token = state
                .downloads
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(m.id)
                .cloned();
            let Some(token) = token else { continue }; // cancelled before it started
            if !store.is_installed(m.id) {
                let emitter = app2.clone();
                let progress = move |p: models::DownloadProgress| {
                    let _ = emitter.emit("models://download", p);
                };
                let _ = store.install(m, token, &progress).await;
            }
            state
                .downloads
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(m.id);
            // A new model may now be the automatic pick: swap it in.
            state.stt.unload();
            let _ = app2.emit("models://changed", ());
        }
        let _ = app2.emit("models://changed", ());
    });
    Ok(())
}

#[tauri::command]
pub fn models_cancel(state: State<'_, AppState>, id: String) {
    if let Some(t) = state
        .downloads
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&id)
    {
        t.cancel();
    }
}

// ---------------------------------------------------------------------------
// GPU pack
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gpu_status(state: State<'_, AppState>) -> GpuStatus {
    let enabled = gpu::is_enabled(&state.paths.data_dir);
    gpu::status(&state.paths.data_dir, &state.hardware, enabled)
}

/// Chooses GPU or CPU. Takes effect after a restart, because the libraries
/// are chosen when the process starts.
#[tauri::command]
pub fn gpu_set_enabled(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    let saved = state
        .settings
        .update(|s| s.device = if enabled { "gpu".into() } else { "cpu".into() })?;
    state.stt.unload();
    let _ = app.emit("settings://changed", &saved);
    let _ = app.emit("gpu://changed", ());
    Ok(())
}

/// Installs the CUDA pack: links the one Open Local Assistant already has, or
/// downloads it. Called only after the user confirmed.
#[tauri::command]
pub async fn gpu_install(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if !state.hardware.has_nvidia() {
        return Err(AppError::Invalid(
            "no NVIDIA GPU was detected; GPU mode needs an NVIDIA card, use CPU instead".into(),
        ));
    }
    let data_dir = state.paths.data_dir.clone();
    if gpu::reusable(&data_dir) {
        let dir = data_dir.clone();
        tokio::task::spawn_blocking(move || gpu::reuse_pack(&dir))
            .await
            .map_err(|e| AppError::Other(e.to_string()))??;
        let _ = state.settings.update(|s| s.device = "gpu".into());
        let _ = app.emit("settings://changed", state.settings.get());
        let _ = app.emit("gpu://changed", ());
        return Ok(());
    }
    let token = {
        let mut d = state.downloads.lock().unwrap_or_else(|p| p.into_inner());
        if d.contains_key(GPU_DOWNLOAD) {
            return Ok(()); // already running
        }
        d.entry(GPU_DOWNLOAD.to_string()).or_default().clone()
    };
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let emitter = app2.clone();
        let progress = move |p: gpu::GpuProgress| {
            let _ = emitter.emit("gpu://download", p);
        };
        let result = gpu::install(&data_dir, token, &progress).await;
        let state = app2.state::<AppState>();
        state
            .downloads
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(GPU_DOWNLOAD);
        if result.is_ok() {
            // Installing it is what the user asked for, so switch it on too.
            let _ = state.settings.update(|s| s.device = "gpu".into());
            let _ = app2.emit("settings://changed", state.settings.get());
        }
        let _ = app2.emit("gpu://changed", ());
    });
    Ok(())
}

#[tauri::command]
pub fn gpu_cancel(state: State<'_, AppState>) {
    if let Some(t) = state
        .downloads
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(GPU_DOWNLOAD)
    {
        t.cancel();
    }
}

/// Starts a fresh copy of the app and quits this one (GPU/CPU switch).
#[tauri::command]
pub fn app_restart(app: AppHandle) -> CmdResult<()> {
    if !gpu::restart() {
        return Err(AppError::Other(
            "the app could not start itself again; quit it from the tray and open it manually"
                .into(),
        ));
    }
    crate::tray::quit(&app);
    Ok(())
}

/// Title-bar buttons of the settings window, which has no system frame.
#[tauri::command]
pub fn settings_minimize(app: AppHandle) {
    if let Some(w) = app.get_webview_window(window::SETTINGS) {
        let _ = w.minimize();
    }
}

/// Closing the settings only hides them; dictation keeps running in the tray.
#[tauri::command]
pub fn settings_close(app: AppHandle) {
    shortcuts::register_all(&app);
    if let Some(w) = app.get_webview_window(window::SETTINGS) {
        let _ = w.hide();
    }
}

// ---------------------------------------------------------------------------
// Dictation overlay
// ---------------------------------------------------------------------------

/// Cancels in-progress dictation (Esc / overlay X): drops audio, skips the
/// insert, shows "cancelled" feedback. No-op when idle.
#[tauri::command]
pub fn dictation_cancel(app: AppHandle) {
    window::cancel_dictation(&app);
}

/// Stops listening; review mode opens a live preview while final transcription runs.
#[tauri::command]
pub fn dictation_insert_now(app: AppHandle, state: State<'_, AppState>, text: String) {
    if state.voice.is_active() {
        if state.settings.get().review_before_insert {
            window::set_overlay_review(&app, true);
            let _ = app.emit(
                "dictation://state",
                serde_json::json!({
                    "state": "reviewing",
                    "text": text,
                    "targetApp": window::dictation_target_name(&app),
                }),
            );
        }
        state.voice.stop(false);
    }
}

/// Inserts the (possibly edited) result the overlay holds in review mode.
#[tauri::command]
pub async fn dictation_confirm(app: AppHandle, text: String) -> CmdResult<()> {
    crate::confirm_review(app, text).await
}

/// Discards the current take (listening or under review) and listens again.
#[tauri::command]
pub async fn dictation_retry(app: AppHandle) {
    crate::retry_dictation(app).await;
}

/// Remembers the language chosen in the dictation overlay.
#[tauri::command]
pub fn dictation_set_language(
    app: AppHandle,
    state: State<'_, AppState>,
    language: String,
) -> CmdResult<()> {
    let saved = state.settings.update(|s| s.language = language)?;
    let _ = app.emit("settings://changed", &saved);
    Ok(())
}

/// Clears the dragged overlay position, reverting to the default spot.
#[tauri::command]
pub fn dictation_reset_overlay_position(app: AppHandle) {
    window::reset_overlay_position(&app);
}

/// Shows the overlay's open language menu (logical x, y, width, height in the
/// window) outside the card, or hides that area again (`None`).
#[tauri::command]
pub fn dictation_overlay_menu(app: AppHandle, menu: Option<[f64; 4]>) {
    window::set_overlay_menu(&app, menu);
}
