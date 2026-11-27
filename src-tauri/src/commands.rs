//! Thin IPC layer: validates input, delegates to the modules.

use crate::audio::devices::{self, AudioDevice};
use crate::errors::AppError;
use crate::gpu::{self, GpuStatus};
use crate::models::{self, ModelEntry};
use crate::settings::Settings;
use crate::state::{AppPaths, AppState};
use crate::{autostart, shortcuts, taskbar, window};
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
        // Pin and mini are owned by the overlay commands, like the position.
        let (pinned, mini, mini_w) = (s.overlay_pinned, s.overlay_mini, s.overlay_mini_width);
        let orb_shift = s.overlay_orb_shift;
        // The taskbar button's position is set by dragging it, like the overlay's.
        let widget_x = s.taskbar_widget_x;
        // Own models are added by their command, like the folder.
        let own_models = s.user_models.clone();
        *s = settings;
        s.user_models = own_models;
        s.taskbar_widget_x = widget_x;
        s.models_dir = dir;
        s.overlay_x = x;
        s.overlay_y = y;
        s.overlay_pinned = pinned;
        s.overlay_mini = mini;
        s.overlay_mini_width = mini_w;
        s.overlay_orb_shift = orb_shift;
    })?;
    if saved.device != before.device {
        state.stt.unload();
    }
    if saved.device != before.device
        || saved.model != before.model
        || saved.language != before.language
        || saved.language_models != before.language_models
        || saved.auto_load_models != before.auto_load_models
    {
        crate::preload_models(&app);
    }
    if saved.shortcut != before.shortcut {
        shortcuts::register_all(&app);
    }
    if saved.start_with_os != before.start_with_os {
        if let Err(e) = autostart::set_enabled(&app, saved.start_with_os) {
            tracing::warn!(error = %e, "could not change the autostart entry");
        }
    }
    if saved.overlay_layouts != before.overlay_layouts
        || saved.overlay_style != before.overlay_style
        || saved.overlay_horizontal_only != before.overlay_horizontal_only
    {
        window::refresh_overlay_layout(&app);
    }
    if saved.taskbar_widget != before.taskbar_widget {
        if saved.taskbar_widget {
            crate::start_taskbar_widget(&app);
        } else {
            taskbar::stop();
            window::sync_overlay_pin(&app);
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

/// Stop flag of the running microphone test (Home page level meter), if any.
static MIC_TEST: std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> =
    std::sync::Mutex::new(None);

#[derive(Clone, Serialize)]
struct MicLevel {
    value: f32,
    bands: Vec<f32>,
}

/// Opens the chosen microphone and emits `mic://level` (meter 0..1 and
/// spectrum bands, about 20 per second) until `mic_test_stop`. Only levels are
/// sent; the audio itself is dropped on the spot. Refused while dictating.
#[tauri::command]
pub fn mic_test_start(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    use crate::audio::capture::{Capture, CaptureEvent};
    use std::sync::atomic::Ordering;
    if state.dictation_busy.load(Ordering::SeqCst) {
        return Err(AppError::Audio(
            "dictation is running; the microphone test starts when it ends".into(),
        ));
    }
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Some(old) = MIC_TEST.lock().unwrap().replace(stop.clone()) {
        old.store(true, Ordering::SeqCst);
    }
    let device = state.settings.get().microphone;
    let (mut capture, rx) = match Capture::start(device.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            MIC_TEST.lock().unwrap().take();
            return Err(e);
        }
    };
    std::thread::Builder::new()
        .name("mic-test".into())
        .spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(CaptureEvent::Level(value, bands)) => {
                        let _ = app.emit("mic://level", MicLevel { value, bands });
                    }
                    Ok(CaptureEvent::Samples(_)) => {}
                    Ok(CaptureEvent::Error(_))
                    | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            capture.stop();
        })
        .map_err(|e| AppError::Audio(e.to_string()))?;
    Ok(())
}

/// Stops the microphone test (idempotent).
#[tauri::command]
pub fn mic_test_stop() {
    if let Some(stop) = MIC_TEST.lock().unwrap().take() {
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Speech models
// ---------------------------------------------------------------------------

/// The speech models that are in memory, and whether each runs on the GPU or
/// the CPU. Empty until start-up preloading or the first take has loaded one.
#[tauri::command]
pub fn stt_loaded(state: State<'_, AppState>) -> Vec<crate::stt::LoadedModel> {
    state.stt.loaded_models()
}

/// Loads a model into memory on request (the Load button). Takes seconds for
/// a big one, so it runs off the UI thread; resolves when it is loaded.
#[tauri::command]
pub async fn stt_load(app: AppHandle, id: String) -> CmdResult<()> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = worker.state::<AppState>();
        state.stt.load_model(&state.settings.get(), &id)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;
    let _ = app.emit("models://changed", ());
    Ok(())
}

/// Frees a loaded model's memory. Nothing on disk is touched; the model loads
/// again when a take needs it.
#[tauri::command]
pub fn stt_unload(app: AppHandle, state: State<'_, AppState>, id: String) {
    state.stt.unload_model(&id);
    let _ = app.emit("models://changed", ());
}

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
            kind: "Whisper".into(),
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
                blurb: if c.added {
                    "Added by you".into()
                } else {
                    "Found in the models folder".into()
                },
                download_bytes: 0,
                installed: true,
                downloading: false,
                custom: true,
                path: Some(c.dir.to_string_lossy().to_string()),
                kind: c.kind.into(),
            }),
    );
    list
}

/// Adds a model folder the user picked: it is linked where it is (never copied
/// or deleted) and listed with the others. The folder must be a sherpa-onnx
/// offline model the app can run (Whisper, Moonshine, transducer, or a
/// single-file CTC / SenseVoice / Paraformer / Dolphin / Omnilingual model).
#[tauri::command]
pub fn models_add_own(
    app: AppHandle,
    state: State<'_, AppState>,
    dir: String,
) -> CmdResult<ModelEntry> {
    let path = std::path::PathBuf::from(dir.trim());
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::Invalid(format!(
            "\"{}\" is not an existing folder; pick the model's folder with the folder picker",
            dir.trim()
        )));
    }
    if crate::stt::engine::detect(&path).is_none() {
        return Err(AppError::Invalid(format!(
            "\"{}\" is not a model this app can run. Pick the folder you unpacked from a sherpa-onnx model download: it needs a tokens.txt and the model's .onnx / .ort files (Whisper, Moonshine, transducer, or a single-file model whose folder name says what it is, like sherpa-onnx-dolphin-...)",
            path.display()
        )));
    }
    let text = path.to_string_lossy().to_string();
    let saved = state.settings.update(|s| {
        if !s.user_models.contains(&text) {
            s.user_models.push(text.clone());
        }
    })?;
    state.models.set_user_dirs(
        saved
            .user_models
            .iter()
            .map(std::path::PathBuf::from)
            .collect(),
    );
    let _ = app.emit("models://changed", ());
    let _ = app.emit("settings://changed", &saved);
    let id = format!("custom:{}", path.display());
    state
        .models
        .custom_models()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| ModelEntry {
            id: c.id,
            name: c.name,
            blurb: "Added by you".into(),
            download_bytes: 0,
            installed: true,
            downloading: false,
            custom: true,
            path: Some(c.dir.to_string_lossy().to_string()),
            kind: c.kind.into(),
        })
        .ok_or_else(|| AppError::Invalid("the model could not be added".into()))
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
    crate::preload_models(&app);
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

/// Maximizes the settings window, or puts it back to its size. Returns whether
/// it is maximized now.
#[tauri::command]
pub fn settings_toggle_maximize(app: AppHandle) -> bool {
    let Some(w) = app.get_webview_window(window::SETTINGS) else {
        return false;
    };
    if w.is_maximized().unwrap_or(false) {
        let _ = w.unmaximize();
    } else {
        let _ = w.maximize();
    }
    w.is_maximized().unwrap_or(false)
}

/// Whether the settings window is maximized (the title bar's button icon).
#[tauri::command]
pub fn settings_is_maximized(app: AppHandle) -> bool {
    app.get_webview_window(window::SETTINGS)
        .and_then(|w| w.is_maximized().ok())
        .unwrap_or(false)
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

/// Stops listening; review mode opens a live preview while final transcription
/// runs, but only when something was heard: with no text there is nothing to
/// edit, so the editor is not shown (the "nothing heard" message follows).
#[tauri::command]
pub fn dictation_insert_now(app: AppHandle, state: State<'_, AppState>, text: String) {
    if state.voice.is_active() {
        if state.settings.get().review_before_insert && !text.trim().is_empty() {
            window::set_overlay_review(&app, true);
            crate::emit_state(
                &app,
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
    crate::preload_models(&app);
    Ok(())
}

/// Opens the settings window (the overlay's gear button).
#[tauri::command]
pub fn settings_open(app: AppHandle) {
    let _ = window::open_settings(&app);
}

/// Pins the overlay (stays on screen, always on top, idle until the shortcut or
/// its mic button is used) or unpins it.
#[tauri::command]
pub fn overlay_set_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    pinned: bool,
) -> CmdResult<()> {
    let saved = state.settings.update(|s| s.overlay_pinned = pinned)?;
    let _ = app.emit("settings://changed", &saved);
    window::sync_overlay_pin(&app);
    Ok(())
}

/// Switches the overlay between its small and full size.
#[tauri::command]
pub fn overlay_set_mini(app: AppHandle, state: State<'_, AppState>, mini: bool) -> CmdResult<()> {
    let saved = state.settings.update(|s| s.overlay_mini = mini)?;
    let _ = app.emit("settings://changed", &saved);
    window::refresh_overlay_layout(&app);
    Ok(())
}

/// Resizes the small overlay's width while its edge is dragged (`commit` false:
/// window only) and saves it when the drag ends. Ignored while the size is locked.
#[tauri::command]
pub fn overlay_resize(
    app: AppHandle,
    state: State<'_, AppState>,
    width: u32,
    commit: bool,
) -> CmdResult<()> {
    if state.settings.get().overlay_size_locked {
        return Ok(());
    }
    let width = width.clamp(
        crate::settings::MINI_MIN_WIDTH,
        crate::settings::MINI_MAX_WIDTH,
    );
    window::resize_overlay_mini(&app, width);
    if commit {
        let saved = state.settings.update(|s| s.overlay_mini_width = width)?;
        let _ = app.emit("settings://changed", &saved);
    }
    Ok(())
}

/// Grows the small overlay so its header content fits (`width` measured by the
/// page, logical px). Never shrinks it, and applies even while the size is
/// locked: a pill too narrow for its own buttons is never wanted. Saved.
#[tauri::command]
pub fn overlay_fit_width(app: AppHandle, state: State<'_, AppState>, width: u32) -> CmdResult<()> {
    let width = width.clamp(
        crate::settings::MINI_MIN_WIDTH,
        crate::settings::MINI_MAX_WIDTH,
    );
    if state.settings.get().overlay_mini_width >= width {
        return Ok(());
    }
    window::resize_overlay_mini(&app, width);
    let saved = state.settings.update(|s| s.overlay_mini_width = width)?;
    let _ = app.emit("settings://changed", &saved);
    Ok(())
}

/// Sets the small overlay's visible shapes (logical x, y, width, height in the
/// window, each fully rounded: the bubble style's mic circle and the pill
/// above it), or lets the card fill its box again (`None`).
#[tauri::command]
pub fn dictation_overlay_card(app: AppHandle, shapes: Option<Vec<[f64; 4]>>) {
    window::set_overlay_card(&app, shapes);
}

/// The overlay's mic button: starts dictation, or stops and inserts when
/// already listening (tap style, like the tray entry).
#[tauri::command]
pub async fn dictation_toggle(app: AppHandle) {
    let _ = tokio::task::spawn_blocking(move || shortcuts::toggle_dictation(&app)).await;
}

/// Saves where the bubble's mic circle sits relative to the window's center
/// (logical px) once a drag ends.
#[tauri::command]
pub fn overlay_set_orb_shift(
    app: AppHandle,
    state: State<'_, AppState>,
    shift: i32,
) -> CmdResult<()> {
    let saved = state.settings.update(|s| s.overlay_orb_shift = shift)?;
    let _ = app.emit("settings://changed", &saved);
    Ok(())
}

/// Clears the dragged overlay position, reverting to the default spot.
#[tauri::command]
pub fn dictation_reset_overlay_position(app: AppHandle) {
    window::reset_overlay_position(&app);
}

/// Shows the small overlay's bubble above the pill (logical x, y, width,
/// height in the window), or hides that area again (`None`).
#[tauri::command]
pub fn dictation_overlay_bubble(app: AppHandle, bubble: Option<[f64; 4]>) {
    window::set_overlay_bubble(&app, bubble);
}

/// Shows the overlay's open language menu (logical x, y, width, height in the
/// window) outside the card, or hides that area again (`None`).
#[tauri::command]
pub fn dictation_overlay_menu(app: AppHandle, menu: Option<[f64; 4]>) {
    window::set_overlay_menu(&app, menu);
}

/// Whether the taskbar button is on the taskbar and, if not, why (for the
/// Settings line). Changes arrive as `taskbar://status`.
#[tauri::command]
pub fn taskbar_widget_status() -> taskbar::Status {
    taskbar::status()
}

/// Forgets where the taskbar button was put: it goes back to the default
/// spot beside the notification area, which becomes its position again.
#[tauri::command]
pub fn taskbar_widget_reset_position(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let saved = state.settings.update(|s| s.taskbar_widget_x = None)?;
    taskbar::set_x(None);
    let _ = app.emit("settings://changed", &saved);
    Ok(())
}

/// Closes the idle overlay the taskbar button's hover opened, once the pointer
/// has left both. A running dictation is left alone.
#[tauri::command]
pub fn overlay_hide_idle(app: AppHandle) {
    window::hide_idle_overlay(&app);
}
