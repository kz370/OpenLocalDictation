use crate::state::AppState;
use crate::window;
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

/// Shows the microphone's state on the tray icon: red, with the problem in the
/// tooltip, while there is no usable microphone; the normal icon otherwise.
pub fn set_mic_status(app: &AppHandle, status: crate::mic::MicStatus) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let Some(normal) = app.default_window_icon() else {
        return;
    };
    let icon = if status.is_ok() {
        normal.clone()
    } else {
        tauri::image::Image::new_owned(tint_red(normal.rgba()), normal.width(), normal.height())
    };
    let _ = tray.set_icon(Some(icon));
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
