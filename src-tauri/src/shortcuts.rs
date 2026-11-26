//! Global shortcuts: the dictation hotkey (hold or toggle), Esc to cancel and
//! the review-mode keys.

use super::window;
use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

fn configured(app: &AppHandle) -> Option<String> {
    let keys = app.state::<AppState>().settings.get().shortcut;
    (!keys.trim().is_empty()).then_some(keys)
}

fn is_modifier_only_shortcut(keys: &str) -> bool {
    let tokens: Vec<&str> = keys
        .split('+')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return false;
    }
    tokens.iter().all(|token| {
        matches!(
            token.to_ascii_uppercase().as_str(),
            "ALT"
                | "OPTION"
                | "CONTROL"
                | "CTRL"
                | "COMMANDORCONTROL"
                | "COMMANDORCTRL"
                | "CMDORCTRL"
                | "CMDORCONTROL"
                | "SHIFT"
                | "SUPER"
                | "META"
                | "COMMAND"
                | "CMD"
                | "WIN"
        )
    })
}

/// Single modifiers (e.g. "Alt") fire on every normal press of that key
/// (Alt+Tab, Alt+F4, menu focus). Require 2+ modifiers for modifier-only.
fn modifier_only_count(keys: &str) -> usize {
    keys.split('+')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .count()
}

fn is_supported_modifier_only(keys: &str) -> bool {
    is_modifier_only_shortcut(keys) && modifier_only_count(keys) >= 2
}

#[cfg(windows)]
fn windows_modifier_pressed(token: &str) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU,
        VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };
    let pressed = |vk: i32| unsafe { (GetAsyncKeyState(vk) as i16) < 0 };
    match token.to_ascii_uppercase().as_str() {
        "ALT" | "OPTION" => {
            pressed(VK_MENU.0 as i32) || pressed(VK_LMENU.0 as i32) || pressed(VK_RMENU.0 as i32)
        }
        "CONTROL" | "CTRL" | "COMMANDORCONTROL" | "COMMANDORCTRL" | "CMDORCTRL"
        | "CMDORCONTROL" => {
            pressed(VK_CONTROL.0 as i32)
                || pressed(VK_LCONTROL.0 as i32)
                || pressed(VK_RCONTROL.0 as i32)
        }
        "SHIFT" => {
            pressed(VK_SHIFT.0 as i32) || pressed(VK_LSHIFT.0 as i32) || pressed(VK_RSHIFT.0 as i32)
        }
        "SUPER" | "META" | "COMMAND" | "CMD" | "WIN" => {
            pressed(VK_LWIN.0 as i32) || pressed(VK_RWIN.0 as i32)
        }
        _ => false,
    }
}

#[cfg(windows)]
fn windows_modifier_shortcut_pressed(keys: &str) -> bool {
    keys.split('+')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .all(windows_modifier_pressed)
}

/// In hold mode a press shorter than this is a tap: recording latches on and
/// the next press stops it.
const TAP_MAX: Duration = Duration::from_millis(400);
static TAP_LATCHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PRESS_AT: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

fn dispatch(app: &AppHandle, pressed: bool) {
    let state = app.state::<AppState>();
    let hold = state.settings.get().mode == "hold";
    let active = state.voice.is_active();
    tracing::info!(
        pressed,
        active,
        busy = state.dictation_busy.load(Ordering::Relaxed),
        transcribing = state.voice.is_transcribing(),
        "dictation shortcut"
    );
    if pressed && active && hold && TAP_LATCHED.swap(false, Ordering::Relaxed) {
        // Recording was latched on by a quick tap: this press ends it.
        state.voice.stop(false);
        return;
    }
    if !pressed && active && hold {
        let held = PRESS_AT
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .map(|t| t.elapsed());
        if held.is_some_and(|d| d < TAP_MAX) {
            // A tap, not a hold: keep listening until the next press.
            TAP_LATCHED.store(true, Ordering::Relaxed);
            return;
        }
    }
    if pressed && !active {
        TAP_LATCHED.store(false, Ordering::Relaxed);
        *PRESS_AT.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
        // The shortcut again while the last take is still being transcribed or
        // waits for review: drop it and start a new recording. Blocking steps
        // run off this thread.
        if state.voice.is_transcribing() {
            state.voice.cancel_transcription();
            let app = app.clone();
            thread::spawn(move || start_dictation(&app));
        } else if window::discard_review(app) {
            let app = app.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(150));
                start_dictation(&app);
            });
        } else {
            start_dictation(app);
        }
    } else if active && ((hold && !pressed) || (!hold && pressed)) {
        state.voice.stop(false);
    }
}

pub fn start_dictation(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.dictation_busy.load(Ordering::Relaxed) {
        tracing::info!("start ignored: the previous take is still being inserted");
        return;
    }
    let generation = state
        .dictation_session_generation
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    // A fresh session owns the cancel flag (clears a stale Esc).
    state.dictation_cancel.store(false, Ordering::Relaxed);
    // No microphone to listen to: say so and stay idle (nothing is started).
    let mic_status = crate::mic::check(state.settings.get().microphone.as_deref());
    if !mic_status.is_ok() {
        let code = match mic_status {
            crate::mic::MicStatus::Muted => "mic_muted",
            crate::mic::MicStatus::Blocked => "mic_blocked",
            _ => "mic_missing",
        };
        tracing::info!(code, "dictation not started: no usable microphone");
        window::show_overlay(app);
        crate::emit_state(
            app,
            serde_json::json!({
                "state": "error",
                "error": { "code": code, "detail": mic_status.problem().unwrap_or_default() }
            }),
        );
        crate::hide_overlay_later(app, crate::LINGER_MSG_MS);
        return;
    }
    window::remember_target(app);
    match state.voice.start() {
        Ok(_) => {
            window::show_overlay(app);
            watch_dictation_target(app.clone(), generation);
        }
        Err(e) => {
            window::show_overlay(app);
            crate::emit_state(app, serde_json::json!({"state": "error", "error": e}));
            crate::hide_overlay_later(app, crate::LINGER_MSG_MS);
        }
    }
}

fn watch_dictation_target(app: AppHandle, generation: u64) {
    thread::spawn(move || {
        let mut last_active = Instant::now();
        let mut publish_initial = true;
        loop {
            let state = app.state::<AppState>();
            if state.dictation_session_generation.load(Ordering::Relaxed) != generation {
                break;
            }
            let reviewing = state
                .dictation_review
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some();
            let active = state.voice.is_active()
                || state.dictation_busy.load(Ordering::Relaxed)
                || reviewing;
            if active {
                last_active = Instant::now();
                let changed = window::refresh_dictation_target(&app);
                if publish_initial || changed {
                    window::publish_dictation_target(&app);
                    publish_initial = false;
                }
            } else if last_active.elapsed() >= Duration::from_secs(2) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
}

/// Tray entry: start dictation, or stop and insert if already listening.
/// Always tap-style, regardless of the hold/toggle shortcut mode.
pub fn toggle_dictation(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.voice.is_active() {
        state.voice.stop(false);
    } else {
        start_dictation(app);
    }
}

#[cfg(windows)]
fn esc_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // VK_ESCAPE; observe only, never swallow.
    unsafe { (GetAsyncKeyState(0x1B) as i16) < 0 }
}

/// One watcher thread: polls the modifier-only hotkey (when that is what is
/// configured) and Esc. Esc cancels dictation anywhere, because the overlay
/// never takes focus while listening. Edge-triggered and observe-only.
#[cfg(windows)]
fn start_watcher(app: &AppHandle) {
    use std::sync::{Mutex, OnceLock};
    static COMBO: OnceLock<std::sync::Arc<Mutex<Option<String>>>> = OnceLock::new();
    static STARTED: OnceLock<()> = OnceLock::new();
    let combo = COMBO
        .get_or_init(|| std::sync::Arc::new(Mutex::new(None)))
        .clone();
    // Refreshed on every register_all (the user may have changed it).
    *combo.lock().unwrap_or_else(|p| p.into_inner()) =
        configured(app).filter(|k| is_supported_modifier_only(k));
    if STARTED.set(()).is_err() {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        let (mut last, mut last_esc) = (false, false);
        loop {
            let keys = combo.lock().map(|g| g.clone()).unwrap_or_default();
            match keys {
                Some(keys) => {
                    let pressed = windows_modifier_shortcut_pressed(&keys);
                    if pressed != last {
                        dispatch(&app, pressed);
                    }
                    last = pressed;
                }
                None => last = false,
            }
            let esc = esc_pressed();
            if esc && !last_esc {
                let st = app.state::<AppState>();
                if st.voice.is_active() || st.dictation_busy.load(Ordering::Relaxed) {
                    window::cancel_dictation(&app);
                }
            }
            last_esc = esc;
            thread::sleep(Duration::from_millis(25));
        }
    });
}

/// Temporarily releases all global shortcuts so the key combination reaches
/// the settings window while the user is recording a new one.
pub fn unregister_all(app: &AppHandle) {
    app.state::<AppState>()
        .review_shortcuts_active
        .store(false, Ordering::Relaxed);
    let _ = app.global_shortcut().unregister_all();
}

/// Replaces configured shortcuts with review actions, so the hotkeys are
/// handled even while the dictated-into application owns keyboard focus.
pub fn register_review_shortcuts(app: &AppHandle) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    app.state::<AppState>()
        .review_shortcuts_active
        .store(false, Ordering::Relaxed);

    for keys in [
        "CommandOrControl+Shift+Backspace",
        "CommandOrControl+Enter",
        "CommandOrControl+Shift+Enter",
    ] {
        let shortcut = match keys.parse::<Shortcut>() {
            Ok(shortcut) => shortcut,
            Err(error) => {
                register_all(app);
                return Err(format!("could not parse review shortcut {keys}: {error}"));
            }
        };
        if let Err(error) = gs.register(shortcut) {
            tracing::warn!(keys, %error, "review shortcut registration failed");
            register_all(app);
            return Err(format!(
                "could not register review shortcut {keys}: {error}"
            ));
        }
    }

    app.state::<AppState>()
        .review_shortcuts_active
        .store(true, Ordering::Relaxed);
    Ok(())
}

/// (Re-)registers the dictation shortcut; a failure (e.g. taken by another
/// app) is recorded for the settings UI instead of aborting.
pub fn register_all(app: &AppHandle) {
    app.state::<AppState>()
        .review_shortcuts_active
        .store(false, Ordering::Relaxed);
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut errors = Vec::new();
    if let Some(keys) = configured(app) {
        if is_modifier_only_shortcut(&keys) {
            if !is_supported_modifier_only(&keys) {
                errors.push(format!("{keys}: single-modifier shortcuts are disabled (use 2+ modifiers or modifier+key)"));
            }
        } else {
            match keys.parse::<Shortcut>() {
                Ok(sc) => {
                    if let Err(e) = gs.register(sc) {
                        tracing::warn!(keys, error = %e, "shortcut registration failed");
                        errors.push(format!("{keys}: {e}"));
                    }
                }
                Err(e) => errors.push(format!("{keys}: {e}")),
            }
        }
    }
    #[cfg(windows)]
    start_watcher(app);
    *app.state::<AppState>()
        .shortcut_errors
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = errors;
}

pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let state = app.state::<AppState>();
    if state.review_shortcuts_active.load(Ordering::Relaxed) {
        for (keys, action) in [
            ("CommandOrControl+Shift+Backspace", "cancel"),
            ("CommandOrControl+Enter", "insert"),
            ("CommandOrControl+Shift+Enter", "retry"),
        ] {
            if keys
                .parse::<Shortcut>()
                .map(|s| s.id() == shortcut.id())
                .unwrap_or(false)
            {
                if event.state() == ShortcutState::Pressed {
                    if let Err(error) = app.emit_to(
                        super::window::OVERLAY,
                        "dictation://review-shortcut",
                        action,
                    ) {
                        tracing::warn!(action, %error, "failed to deliver dictation review shortcut");
                    }
                }
                return;
            }
        }
    }

    let matches_configured = configured(app).is_some_and(|keys| {
        !is_modifier_only_shortcut(&keys)
            && keys
                .parse::<Shortcut>()
                .map(|s| s.id() == shortcut.id())
                .unwrap_or(false)
    });
    if matches_configured {
        dispatch(app, event.state() == ShortcutState::Pressed);
    }
}

#[cfg(test)]
mod tests {
    use super::{is_modifier_only_shortcut, is_supported_modifier_only};

    #[test]
    fn single_modifier_not_supported() {
        assert!(is_modifier_only_shortcut("Alt"));
        assert!(!is_supported_modifier_only("Alt"));
        assert!(is_supported_modifier_only("CommandOrControl+Alt"));
        assert!(!is_supported_modifier_only("CommandOrControl+Space"));
    }
}
