//! Open Local Dictation: hold a hotkey, speak, and the text appears in the
//! application you were using. Speech recognition is Whisper, run on this PC.

pub mod audio;
pub mod autostart;
pub mod commands;
pub mod errors;
pub mod gpu;
pub mod hardware;
pub mod logging;
pub mod mic;
pub mod models;
pub mod settings;
pub mod shortcuts;
pub mod state;
pub mod stt;
pub mod taskbar;
pub mod tray;
pub mod typing;
pub mod window;

use models::ModelStore;
use settings::SettingsStore;
use state::{AppPaths, AppState, ReviewPending};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use stt::session::{VoiceEvent, VoiceSessions};
use stt::SttService;
use tauri::{AppHandle, Emitter, Listener, Manager};

/// How long the overlay lingers after a dictation ends: a quick glance at
/// "Inserted", longer for a message that has to be read.
pub(crate) const LINGER_OK_MS: u64 = 800;
pub(crate) const LINGER_MSG_MS: u64 = 2200;

/// Sends a `dictation://state` event to the pages and keeps the taskbar button
/// (when there is one) showing the same state.
pub(crate) fn emit_state(app: &AppHandle, payload: serde_json::Value) {
    if let Some(state) = payload
        .get("state")
        .and_then(|s| s.as_str())
        .and_then(taskbar::WidgetState::from_dictation)
    {
        taskbar::set_state(state);
    }
    let _ = app.emit("dictation://state", payload);
}

/// Starts the taskbar mic button. Its callbacks run on the widget's own
/// thread, so each one only hands work to the app and returns.
pub(crate) fn start_taskbar_widget(app: &AppHandle) {
    use taskbar::Event;
    let events = app.clone();
    let status = app.clone();
    taskbar::start(taskbar::Config {
        x: app.state::<AppState>().settings.get().taskbar_widget_x,
        language: app.state::<AppState>().settings.get().language,
        on_event: Arc::new(move |ev| {
            let app = events.clone();
            match ev {
                Event::Clicked => {
                    tauri::async_runtime::spawn_blocking(move || taskbar_click(&app));
                }
                Event::HoverStart => {
                    window::show_idle_overlay(&app);
                    let _ = app.emit("overlay://widget-hover", true);
                }
                Event::HoverEnd => {
                    let _ = app.emit("overlay://widget-hover", false);
                }
                Event::ContextMenu => {
                    // A menu opens just above the indicator and blocks until it
                    // closes: not on the widget's thread.
                    tauri::async_runtime::spawn_blocking(move || {
                        let current = app.state::<AppState>().settings.get().language;
                        if let Some(code) = taskbar::pick_language(&current) {
                            let _ = set_language(&app, code);
                        }
                    });
                }
                Event::Moved(x) => {
                    // Writes a file: not on the widget's thread.
                    tauri::async_runtime::spawn_blocking(move || {
                        let saved = app
                            .state::<AppState>()
                            .settings
                            .update(|s| s.taskbar_widget_x = Some(x));
                        if let Ok(saved) = saved {
                            let _ = app.emit("settings://changed", &saved);
                        }
                    });
                }
            }
        }),
        on_status: Arc::new(move |s| {
            let _ = status.emit("taskbar://status", &s);
            // The widget replaces the floating circle: a pinned overlay goes
            // away while it is there and comes back when it is not.
            window::sync_overlay_pin(&status);
        }),
    });
}

/// Remembers the dictation language and loads its models. Mid-recording the
/// recognizer is already built, so a running take restarts on the new language.
pub(crate) fn set_language(app: &AppHandle, language: String) -> Result<(), errors::AppError> {
    let state = app.state::<AppState>();
    let saved = state.settings.update(|s| s.language = language)?;
    let _ = app.emit("settings://changed", &saved);
    preload_models(app);
    if state.voice.is_active() {
        tauri::async_runtime::spawn(retry_dictation(app.clone()));
    }
    Ok(())
}

/// Watches the microphone for the life of the app: when it is unplugged,
/// muted or blocked the tray icon and the taskbar button turn red.
pub(crate) fn start_mic_watch(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("mic-watch".into())
        .spawn(move || {
            let mut last = None;
            loop {
                let device = app.state::<AppState>().settings.get().microphone;
                let status = mic::check(device.as_deref());
                if last != Some(status) {
                    last = Some(status);
                    tracing::info!(?status, "microphone status");
                    taskbar::set_mic_ok(status.is_ok());
                    tray::set_mic_status(&app, status);
                    let _ = app.emit("mic://status", status);
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
    if spawned.is_err() {
        tracing::warn!("could not start the microphone watcher");
    }
}

/// A click on the taskbar button: focuses a result waiting for review, ignores
/// a take that is being transcribed, otherwise starts or stops dictation.
fn taskbar_click(app: &AppHandle) {
    let state = app.state::<AppState>();
    let reviewing = state
        .dictation_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_some();
    if reviewing {
        window::focus_overlay(app);
    } else if !state.voice.is_transcribing() {
        shortcuts::toggle_dictation(app);
    }
}

/// Loads the speech models for the current language and the default one on a
/// background thread, so the first dictation does not wait for them.
pub fn preload_models(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        state.stt.preload(&state.settings.get());
        let _ = app.emit("models://changed", ());
    });
}

fn init_state(app: &AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let paths = AppPaths {
        logs_dir: app
            .path()
            .app_log_dir()
            .unwrap_or_else(|_| data_dir.join("logs")),
        data_dir,
    };
    std::fs::create_dir_all(&paths.data_dir)?;

    let settings = Arc::new(SettingsStore::load(&paths.data_dir));
    let hardware = hardware::detect();
    tracing::info!(
        cores = hardware.logical_cores,
        gpus = hardware.gpus.len(),
        "hardware detected"
    );

    // The user may have pointed the models at a folder shared with other apps.
    let own_models = paths.data_dir.join("models");
    let models_root = match settings.get().models_dir.trim() {
        "" => own_models.clone(),
        dir => std::path::PathBuf::from(dir),
    };
    let models = Arc::new(ModelStore::with_vad_dir(
        models_root,
        own_models.join("vad"),
    ));
    models.set_user_dirs(
        settings
            .get()
            .user_models
            .iter()
            .map(std::path::PathBuf::from)
            .collect(),
    );
    let vad_path = models.install_bundled_vad()?;
    let stt = Arc::new(SttService::new(models.clone(), hardware.clone()));
    let handle = app.clone();
    let voice = Arc::new(VoiceSessions::new(
        stt.clone(),
        settings.clone(),
        vad_path,
        Arc::new(move |ev: VoiceEvent| on_voice_event(&handle, ev)),
    ));

    Ok(AppState {
        paths,
        settings,
        models,
        stt,
        voice,
        hardware,
        downloads: Mutex::new(Default::default()),
        dictation_busy: AtomicBool::new(false),
        dictation_cancel: AtomicBool::new(false),
        dictation_session_generation: AtomicU64::new(0),
        live_typer: Default::default(),
        dictation_review: Mutex::new(None),
        dictation_target: AtomicIsize::new(0),
        review_shortcuts_active: AtomicBool::new(false),
        shortcut_errors: Mutex::new(Vec::new()),
    })
}

/// Routes voice events: the transcript is handled natively, everything else is
/// forwarded to the overlay.
fn on_voice_event(app: &AppHandle, ev: VoiceEvent) {
    let state = app.state::<AppState>();
    match &ev {
        VoiceEvent::Transcript { text, .. } => {
            let (text, app) = (text.clone(), app.clone());
            tauri::async_runtime::spawn(async move { run_dictation(app, text).await });
        }
        VoiceEvent::State { state: s, .. } => {
            if s == "listening" {
                // A new session must not inherit live-typed text from the last one.
                state.live_typer.reset();
            }
            // A stop arriving after Esc-cancel must not resurrect the overlay as
            // idle; report cancelled instead (the flag stays for run_dictation).
            if s == "idle" && state.dictation_cancel.load(Ordering::Relaxed) {
                emit_state(app, serde_json::json!({ "state": "cancelled" }));
                // Back to the idle card (pinned) or away (not pinned).
                hide_overlay_later(app, LINGER_MSG_MS);
            } else {
                emit_state(app, serde_json::json!({ "state": s }));
            }
        }
        VoiceEvent::Partial { text } => {
            // Review mode types nothing until the user confirms the result.
            let d = state.settings.get();
            if d.insert_method == "type" && !d.review_before_insert {
                state.live_typer.set_target(app.clone(), text.clone());
            }
        }
        VoiceEvent::Error { code, detail } => {
            retract_in_background(app);
            emit_state(
                app,
                serde_json::json!({ "state": "error", "error": { "code": code, "detail": detail } }),
            );
            hide_overlay_later(app, LINGER_MSG_MS);
        }
        VoiceEvent::Level { .. } => {}
    }
    // Levels and live text go to the overlay only.
    if matches!(&ev, VoiceEvent::Level { .. } | VoiceEvent::Partial { .. }) {
        let _ = app.emit_to(window::OVERLAY, "voice://event", ev);
    }
}

fn retract_in_background(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = app.state::<AppState>().live_typer.retract() {
            tracing::warn!(error = %e, "could not retract live-typed text");
        }
    });
}

pub(crate) fn hide_overlay_later(app: &AppHandle, ms: u64) {
    let generation = app
        .state::<AppState>()
        .dictation_session_generation
        .load(Ordering::Relaxed);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        window::hide_overlay_if_session_idle(&app, generation);
    });
}

/// Erases whatever dictation has live-typed so far, on a blocking thread.
async fn retract_live_typed(app: &AppHandle) {
    let app = app.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = app.state::<AppState>().live_typer.retract() {
            tracing::warn!(error = %e, "could not retract live-typed text");
        }
    })
    .await;
}

async fn run_dictation(app: AppHandle, raw: String) {
    let state = app.state::<AppState>();
    state.dictation_busy.store(true, Ordering::Relaxed);
    // Esc/X cancel wins over any pending transcript.
    if state.dictation_cancel.swap(false, Ordering::Relaxed) {
        retract_live_typed(&app).await;
        emit_state(&app, serde_json::json!({ "state": "cancelled" }));
        state.dictation_busy.store(false, Ordering::Relaxed);
        hide_overlay_later(&app, LINGER_MSG_MS);
        return;
    }
    let raw = raw.trim().to_string();

    let linger = if raw.is_empty() {
        retract_live_typed(&app).await;
        // A live preview may have opened the editor (and taken focus) on text
        // that turned out empty: close it and hand focus back.
        window::set_overlay_review(&app, false);
        window::restore_target(&app);
        emit_state(&app, serde_json::json!({ "state": "empty" }));
        LINGER_MSG_MS
    } else if state.settings.get().review_before_insert {
        // Hold the result for editing. Stays "busy" until confirmed, retried or cancelled.
        *state
            .dictation_review
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(ReviewPending { raw: raw.clone() });
        window::set_overlay_review(&app, true);
        emit_state(
            &app,
            serde_json::json!({
                "state": "review",
                "result": { "raw": raw, "inserted": raw },
                "targetApp": window::dictation_target_name(&app),
            }),
        );
        return;
    } else {
        if insert_and_report(&app, raw, true).await {
            LINGER_OK_MS
        } else {
            LINGER_MSG_MS
        }
    };
    state.dictation_busy.store(false, Ordering::Relaxed);
    hide_overlay_later(&app, linger);
}

/// Types or pastes the final text and reports the outcome to the overlay.
/// `live` means partials were typed while speaking. Returns whether the text
/// was inserted.
async fn insert_and_report(app: &AppHandle, text: String, live: bool) -> bool {
    let state = app.state::<AppState>();
    let settings = state.settings.get();
    let final_text = typing::finalize_text(&text, &settings);
    let insert = if live && settings.insert_method == "type" {
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || app2.state::<AppState>().live_typer.finish(&final_text))
            .await
    } else {
        tokio::task::spawn_blocking(move || typing::insert_text(&final_text, &settings)).await
    };
    match insert {
        Ok(Ok(())) => {
            emit_state(
                app,
                serde_json::json!({ "state": "inserted", "result": { "raw": text, "inserted": text } }),
            );
            true
        }
        Ok(Err(e)) => {
            emit_state(app, serde_json::json!({ "state": "error", "error": e }));
            false
        }
        Err(e) => {
            emit_state(
                app,
                serde_json::json!({ "state": "error", "error": { "code": "other", "detail": e.to_string() } }),
            );
            false
        }
    }
}

/// Inserts the (possibly edited) result the overlay is holding for review.
pub(crate) async fn confirm_review(app: AppHandle, text: String) -> Result<(), errors::AppError> {
    let state = app.state::<AppState>();
    state
        .dictation_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .ok_or_else(|| errors::AppError::Invalid("nothing is waiting to be inserted".into()))?;
    let text = text.trim().to_string();
    // The overlay took focus for editing: shrink it, give focus back to the
    // window that was being dictated into, and let that settle before typing.
    window::set_overlay_review(&app, false);
    window::restore_target(&app);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let linger = if text.is_empty() {
        emit_state(&app, serde_json::json!({ "state": "empty" }));
        LINGER_MSG_MS
    } else if insert_and_report(&app, text, false).await {
        LINGER_OK_MS
    } else {
        LINGER_MSG_MS
    };
    state.dictation_busy.store(false, Ordering::Relaxed);
    hide_overlay_later(&app, linger);
    Ok(())
}

/// Throws away what was heard (or is still being heard) and listens again.
pub(crate) async fn retry_dictation(app: AppHandle) {
    let state = app.state::<AppState>();
    if window::discard_review(&app) {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    } else if state.voice.is_active() {
        state.voice.stop(true);
        // Anything live-typed so far belongs to the take being thrown away.
        retract_live_typed(&app).await;
    } else {
        return; // transcribing: nothing to retry yet
    }
    let _ = tokio::task::spawn_blocking(move || shortcuts::start_dictation(&app)).await;
}

/// `--delay <ms>`: a restart waits for the previous process to let go of the
/// single-instance lock.
fn startup_delay() -> Option<std::time::Duration> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--delay")?;
    let ms = args.get(i + 1)?.parse::<u64>().ok()?;
    Some(std::time::Duration::from_millis(ms.min(10_000)))
}

pub fn run() {
    // Before Tauri (and its single-instance guard) starts: when the GPU pack is
    // installed and switched on, this relaunches the app with the pack's
    // libraries ahead of the bundled CPU ones.
    if gpu::activate_early() {
        return;
    }
    if let Some(d) = startup_delay() {
        std::thread::sleep(d);
    }
    tauri::Builder::default()
        // Only one instance runs: a second launch opens the settings.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = window::open_settings(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(shortcuts::handle)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let log_dir = handle
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("local-dictation-logs"));
            if let Some(guard) = logging::init(&log_dir) {
                // Keep the non-blocking writer alive for the whole process.
                Box::leak(Box::new(guard));
            }
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "starting Open Local Dictation"
            );

            let state = init_state(&handle)?;
            app.manage(state);
            let (start_with_os, ready) = {
                let st = handle.state::<AppState>();
                let s = st.settings.get();
                (s.start_with_os, st.stt.is_ready(&s))
            };
            autostart::sync(&handle, start_with_os);
            tray::create(&handle)?;
            start_mic_watch(&handle);
            shortcuts::register_all(&handle);
            let _ = window::create_overlay(&handle);
            // Every settings change passes through this event: the taskbar
            // button shows the language that is set.
            handle.listen("settings://changed", |ev| {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(ev.payload()) {
                    if let Some(code) = v.get("language").and_then(|l| l.as_str()) {
                        taskbar::set_language(code);
                    }
                }
            });
            let settings = handle.state::<AppState>().settings.get();
            if settings.taskbar_widget {
                // A pinned overlay waits for the widget's status: it only shows
                // when the button could not be placed.
                start_taskbar_widget(&handle);
            } else if settings.overlay_pinned {
                window::show_overlay(&handle);
            }
            // Pre-create settings hidden: on-demand creation flakes on some
            // machines while startup-created webviews always work.
            let _ = window::ensure_settings(&handle);
            // Paint the overlay's page once while invisible, so the tray's first click shows no bare frame.
            window::warm_up_overlay(&handle);
            preload_models(&handle);
            // Nothing to listen with yet: open the settings so a model can be chosen.
            if !ready {
                let _ = window::open_settings(&handle);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::shortcut_errors,
            commands::shortcuts_capture,
            commands::app_info,
            commands::open_logs,
            commands::audio_inputs,
            commands::mic_test_start,
            commands::mic_test_stop,
            commands::models_list,
            commands::stt_loaded,
            commands::stt_load,
            commands::stt_unload,
            commands::models_add_own,
            commands::models_download,
            commands::models_cancel,
            commands::models_set_dir,
            commands::gpu_status,
            commands::gpu_set_enabled,
            commands::gpu_install,
            commands::gpu_cancel,
            commands::app_restart,
            commands::settings_minimize,
            commands::settings_toggle_maximize,
            commands::settings_is_maximized,
            commands::settings_close,
            commands::dictation_cancel,
            commands::dictation_insert_now,
            commands::dictation_confirm,
            commands::dictation_retry,
            commands::dictation_set_language,
            commands::dictation_reset_overlay_position,
            commands::overlay_set_orb_shift,
            commands::dictation_overlay_menu,
            commands::dictation_overlay_bubble,
            commands::settings_open,
            commands::overlay_set_pinned,
            commands::overlay_set_mini,
            commands::overlay_resize,
            commands::overlay_fit_width,
            commands::dictation_overlay_card,
            commands::dictation_toggle,
            commands::taskbar_widget_status,
            commands::taskbar_widget_reset_position,
            commands::taskbar_set_languages,
            commands::overlay_hide_idle,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Open Local Dictation")
        .run(|_app, event| {
            // Keep running in the tray when all windows are closed.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
