use crate::state::AppState;
use crate::window;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let dictate = MenuItem::with_id(app, "dictate", "Show dictation window", true, None::<&str>)?;
    let close = MenuItem::with_id(app, "close", "Hide dictation window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &dictate,
            &close,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Open Local Dictation")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "dictate" => window::open_overlay_from_tray(app),
            "close" => {
                // Same as the overlay's X: unpin, then it goes away (a running dictation is left alone).
                let state = app.state::<AppState>();
                if let Ok(saved) = state.settings.update(|s| s.overlay_pinned = false) {
                    let _ = app.emit("settings://changed", &saved);
                }
                window::sync_overlay_pin(app);
            }
            "settings" => {
                let _ = window::open_settings(app);
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::open_overlay_from_tray(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

pub fn quit(app: &AppHandle) {
    app.state::<AppState>().voice.stop(true);
    for label in [window::OVERLAY, window::SETTINGS] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    app.exit(0);
}

/// The tray icon in red: the app icon's teal turned to red, its white kept.
/// Pure so it can be tested; `rgba` is 4 bytes per pixel.
fn tint_red(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        // 0 for the icon's teal (its weakest channel is low), 1 for white.
        let min = px[0].min(px[1]).min(px[2]) as f32 / 255.0;
        let white = ((min - 0.08) / 0.92).clamp(0.0, 1.0);
        // Brightness of the teal carries over, so the shading survives.
        let lum = (0.3 * px[0] as f32 + 0.59 * px[1] as f32 + 0.11 * px[2] as f32) / 255.0;
        let red = [
            (150.0 + 90.0 * lum).min(255.0),
            (24.0 + 30.0 * lum).min(255.0),
            (24.0 + 30.0 * lum).min(255.0),
        ];
        for c in 0..3 {
            px[c] = (red[c] * (1.0 - white) + 255.0 * white).round() as u8;
        }
    }
    out
}

static MIC_OK: AtomicBool = AtomicBool::new(true);
/// The color profile's index in `profile::PROFILES`.
static PROFILE: AtomicUsize = AtomicUsize::new(0);

/// The app icon in the color profile's color (never red): the icon of the
/// app's windows, which is what the Windows taskbar and Alt+Tab show.
pub fn profile_icon(app: &AppHandle) -> Option<tauri::image::Image<'static>> {
    let normal = app.default_window_icon()?;
    let rgba = match crate::profile::PROFILES[PROFILE.load(Ordering::Relaxed)].tray {
        Some(tint) => crate::profile::tint_icon(normal.rgba(), tint),
        None => normal.rgba().to_vec(),
    };
    Some(tauri::image::Image::new_owned(
        rgba,
        normal.width(),
        normal.height(),
    ))
}

/// Sets the tray icon: red while there is no usable microphone (whatever the
/// color profile), otherwise the app icon in the profile's color. The windows'
/// icons follow the profile too (they stay the profile's color: red is the
/// microphone alert of the tray icon and the taskbar button).
fn refresh_icon(app: &AppHandle) {
    for w in app.webview_windows().values() {
        if let Some(icon) = profile_icon(app) {
            let _ = w.set_icon(icon);
        }
    }
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let Some(normal) = app.default_window_icon() else {
        return;
    };
    let icon = if MIC_OK.load(Ordering::Relaxed) {
        profile_icon(app)
    } else {
        Some(tauri::image::Image::new_owned(
            tint_red(normal.rgba()),
            normal.width(),
            normal.height(),
        ))
    };
    let _ = tray.set_icon(icon);
}

/// Sets the color profile (`profile::PROFILES` id) the tray icon is drawn in.
pub fn set_profile(app: &AppHandle, id: &str) {
    let next = crate::profile::index(id);
    if PROFILE.swap(next, Ordering::Relaxed) != next {
        refresh_icon(app);
    }
}

/// Shows the microphone's state on the tray icon: red, with the problem in the
/// tooltip, while there is no usable microphone; the normal icon otherwise.
pub fn set_mic_status(app: &AppHandle, status: crate::mic::MicStatus) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    MIC_OK.store(status.is_ok(), Ordering::Relaxed);
    refresh_icon(app);
    let tip = match status.problem() {
        Some(p) => format!("Open Local Dictation - {p}"),
        None => "Open Local Dictation".to_string(),
    };
    let _ = tray.set_tooltip(Some(tip));
}

#[cfg(test)]
mod tests {
    use super::tint_red;

    #[test]
    fn teal_turns_red_and_white_stays_white() {
        let out = tint_red(&[15, 118, 110, 255, 255, 255, 255, 255, 0, 0, 0, 0]);
        // Teal: red now dominates.
        assert!(out[0] > out[1] && out[0] > out[2]);
        assert_eq!(out[3], 255);
        // White unchanged, transparent pixels keep their alpha.
        assert_eq!(&out[4..8], &[255, 255, 255, 255]);
        assert_eq!(out[11], 0);
    }
}
