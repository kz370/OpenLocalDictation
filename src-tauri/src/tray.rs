use crate::state::AppState;
use crate::{shortcuts, window};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let title = MenuItem::with_id(app, "title", "Open Local Dictation", false, None::<&str>)?;
    let dictate = MenuItem::with_id(app, "dictate", "Dictate", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &title,
            &dictate,
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
            "dictate" => shortcuts::toggle_dictation(app),
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
                let _ = window::open_settings(tray.app_handle());
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
