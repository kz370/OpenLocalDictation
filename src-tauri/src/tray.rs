use crate::state::AppState;
use crate::window;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let title = MenuItem::with_id(app, "title", "Open Local Dictation", false, None::<&str>)?;
    let dictate = MenuItem::with_id(app, "dictate", "Show dictation window", true, None::<&str>)?;
    let close = MenuItem::with_id(app, "close", "Hide dictation window", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &title,
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
            "dictate" => window::show_idle_overlay(app),
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
                window::show_idle_overlay(tray.app_handle());
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
