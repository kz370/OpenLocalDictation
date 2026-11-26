//! Window management: the dictation overlay (the card shown while dictating)
//! and the settings window.

use crate::state::AppState;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const SETTINGS: &str = "settings";
pub const OVERLAY: &str = "overlay";

/// Logical size of the settings window; it never changes.
const SETTINGS_W: f64 = 780.0;
const SETTINGS_H: f64 = 640.0;

static SUPPRESS_UNTIL: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn suppress_persistence() {
    SUPPRESS_UNTIL.store(now_ms() + 700, Ordering::Relaxed);
}

/// Clamps a window rectangle into the work area so it is always fully visible.
pub fn clamp_into(
    work_pos: (i32, i32),
    work_size: (u32, u32),
    pos: (i32, i32),
    win: (u32, u32),
) -> (i32, i32) {
    let max_x = work_pos.0 + work_size.0 as i32 - win.0 as i32;
    let max_y = work_pos.1 + work_size.1 as i32 - win.1 as i32;
    (
        pos.0.clamp(work_pos.0, max_x.max(work_pos.0)),
        pos.1.clamp(work_pos.1, max_y.max(work_pos.1)),
    )
}

fn on_any_monitor(window: &WebviewWindow, x: i32, y: i32) -> bool {
    window
        .available_monitors()
        .map(|ms| {
            ms.iter().any(|m| {
                let p = m.position();
                let s = m.size();
                x >= p.x - 50
                    && y >= p.y - 50
                    && x < p.x + s.width as i32 - 50
                    && y < p.y + s.height as i32 - 50
            })
        })
        .unwrap_or(false)
}

/// Creates the settings window hidden (idempotent). Called at startup:
/// on-demand creation flakes on some machines while startup-created webviews
/// always work.
pub fn ensure_settings(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(
        app,
        SETTINGS,
        WebviewUrl::App("index.html#/settings".into()),
    )
    .title("Open Local Dictation")
    // Fixed size and no system frame: the page draws its own title bar.
    .inner_size(SETTINGS_W, SETTINGS_H)
    .resizable(false)
    .maximizable(false)
    .decorations(false)
    .center()
    .visible(false)
    .build()?;
    // Hide instead of destroying: rebuilding the window can render blank, and
    // a close mid-shortcut-recording must still re-register the hotkey.
    let h = app.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            crate::shortcuts::register_all(&h);
            if let Some(w) = h.get_webview_window(SETTINGS) {
                let _ = w.hide();
            }
        }
    });
    Ok(w)
}

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    let w = ensure_settings(app)?;
    let _ = w.unminimize();
    w.show()?;
    w.set_focus()?;
    Ok(())
}

/// Logical size of the dictation overlay card. Listening and review share it,
/// so the card does not jump when a result opens for editing.
const OVERLAY_W: f64 = 480.0;
const OVERLAY_H: f64 = 176.0;
/// Height of the small overlay; its width is `MINI_WIDTH` (a setting, dragged
/// on its right edge).
const MINI_H: f64 = 68.0;
/// Gap between the card and the taskbar at the default spot (logical px).
const OVERLAY_BOTTOM_GAP: f64 = 6.0;

/// Whether the overlay window currently has the small layout.
static MINI: AtomicBool = AtomicBool::new(false);
/// Current width (logical px) of the small layout.
static MINI_WIDTH: AtomicU32 = AtomicU32::new(400);

/// Logical size of the card box in the current layout.
fn card_dims() -> (f64, f64) {
    if MINI.load(Ordering::Relaxed) {
        (MINI_WIDTH.load(Ordering::Relaxed) as f64, MINI_H)
    } else {
        (OVERLAY_W, OVERLAY_H)
    }
}

/// Small layout wanted right now. Review stays small too: its editor floats
/// above the pill.
fn wants_mini(app: &AppHandle) -> bool {
    app.state::<AppState>().settings.get().effective_mini()
}

#[cfg(windows)]
fn hwnd_of(raw: isize) -> windows::Win32::Foundation::HWND {
    windows::Win32::Foundation::HWND(raw as *mut core::ffi::c_void)
}

/// The window that currently has keyboard focus (0 when unknown).
#[cfg(windows)]
fn foreground_window() -> isize {
    // SAFETY: GetForegroundWindow has no preconditions.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize }
}

#[cfg(not(windows))]
fn foreground_window() -> isize {
    0
}

/// Brings a window to the foreground. Windows only lets the process that got
/// the last input do this, so the input queues are attached for the call.
#[cfg(windows)]
fn focus_hwnd(raw: isize) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
        SetForegroundWindow,
    };
    if raw == 0 {
        return;
    }
    let target = hwnd_of(raw);
    // SAFETY: plain Win32 calls on a handle that is validated with IsWindow.
    unsafe {
        if !IsWindow(Some(target)).as_bool() {
            return;
        }
        let (this, other) = (
            GetCurrentThreadId(),
            GetWindowThreadProcessId(GetForegroundWindow(), None),
        );
        let attached =
            other != 0 && other != this && AttachThreadInput(this, other, true).as_bool();
        let _ = BringWindowToTop(target);
        let _ = SetForegroundWindow(target);
        if attached {
            let _ = AttachThreadInput(this, other, false);
        }
        let _ = SetFocus(Some(target));
    }
}

#[cfg(not(windows))]
fn focus_hwnd(_raw: isize) {}

fn overlay_hwnd(app: &AppHandle) -> isize {
    app.get_webview_window(OVERLAY)
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0)
}

fn is_own_window(app: &AppHandle, raw: isize) -> bool {
    [SETTINGS, OVERLAY].iter().any(|label| {
        app.get_webview_window(label)
            .and_then(|window| window.hwnd().ok())
            .is_some_and(|hwnd| hwnd.0 as isize == raw)
    })
}

/// Captures the newly focused external window, ignoring this application's
/// own surfaces. Returns whether the destination handle changed.
pub fn refresh_dictation_target(app: &AppHandle) -> bool {
    let foreground = foreground_window();
    if foreground == 0 || is_own_window(app, foreground) {
        return false;
    }
    app.state::<AppState>()
        .dictation_target
        .swap(foreground, Ordering::Relaxed)
        != foreground
}

/// Publishes the current destination app to the dictation overlay.
pub fn publish_dictation_target(app: &AppHandle) {
    let _ = app.emit(
        "dictation://target",
        serde_json::json!({ "targetApp": dictation_target_name(app) }),
    );
}

/// Remembers the external window focused when dictation starts, clearing stale
/// destinations if dictation was started from one of this app's own windows.
pub fn remember_target(app: &AppHandle) {
    let foreground = foreground_window();
    let target = if foreground != 0 && !is_own_window(app, foreground) {
        foreground
    } else {
        0
    };
    app.state::<AppState>()
        .dictation_target
        .store(target, Ordering::Relaxed);
}

/// Gives keyboard focus back to the window dictation started in.
pub fn restore_target(app: &AppHandle) {
    refresh_dictation_target(app);
    focus_hwnd(
        app.state::<AppState>()
            .dictation_target
            .load(Ordering::Relaxed),
    );
}

/// Returns the remembered target process's friendly app name when available.
#[cfg(windows)]
pub fn dictation_target_name(app: &AppHandle) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

    let raw = app
        .state::<AppState>()
        .dictation_target
        .load(Ordering::Relaxed);
    if raw == 0 {
        return None;
    }
    let hwnd = hwnd_of(raw);
    // SAFETY: the handle is checked before querying its owner and process path.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut image = vec![0u16; 32_768];
        let mut length = image.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(image.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        if result.is_err() || length == 0 {
            return None;
        }
        let executable = String::from_utf16_lossy(&image[..length as usize]);
        let stem = std::path::Path::new(&executable)
            .file_stem()
            .and_then(|name| name.to_str())?;
        Some(application_display_name(stem))
    }
}

#[cfg(not(windows))]
pub fn dictation_target_name(_app: &AppHandle) -> Option<String> {
    None
}

fn application_display_name(executable_stem: &str) -> String {
    match executable_stem.to_ascii_lowercase().as_str() {
        "code" => "Visual Studio Code".into(),
        "notepad" => "Notepad".into(),
        "notepad++" => "Notepad++".into(),
        "winword" => "Microsoft Word".into(),
        "excel" => "Microsoft Excel".into(),
        "powerpnt" => "Microsoft PowerPoint".into(),
        "chrome" => "Google Chrome".into(),
        "msedge" => "Microsoft Edge".into(),
        "firefox" => "Firefox".into(),
        _ => executable_stem.to_owned(),
    }
}

/// Invisible room kept above and below the overlay card (logical px) so the
/// language menu can hang outside the card without the window ever moving or
/// resizing while it is on screen: a move always shows the card jumping for a
/// frame or two before the webview catches up. The room is cut away with a
/// window region (not drawn, clicks pass through); the open menu is added to
/// the region. Fits the menu's 8 rows plus its gap from the pill.
const MENU_ROOM: f64 = 240.0;

/// The open language menu's rectangle in the window (logical x, y, w, h).
static MENU_RECT: Mutex<Option<[f64; 4]>> = Mutex::new(None);
/// The tooltip's rectangle, kept apart so it never replaces the menu or the
/// subtitle bubble.
static TIP_RECT: Mutex<Option<[f64; 4]>> = Mutex::new(None);
/// Card margin inside the window and corner radius, the menu's corner radius,
/// and the room left around the menu for its shadow (logical px; must match
/// .overlay-card / .overlay-lang-menu).
const CARD_MARGIN: f64 = 8.0;
const CARD_RADIUS: f64 = 18.0;
const APP_TAB_HEIGHT: f64 = 48.0;
const APP_TAB_WIDTH_INSET: f64 = 36.0;
const APP_TAB_OVERLAP: f64 = 4.0;
const MENU_RADIUS: f64 = 10.0;
const MENU_SHADOW: f64 = 14.0;

fn room_px(w: &WebviewWindow) -> i32 {
    (MENU_ROOM * w.scale_factor().unwrap_or(1.0)).round() as i32
}

/// Window size for the current layout.
fn overlay_window_size() -> LogicalSize<f64> {
    let (w, h) = card_dims();
    LogicalSize::new(w, h + 2.0 * MENU_ROOM)
}

/// Frameless windows keep their caption styles (for snapping and animations)
/// and merely hide the frame. Once a window region is set, Windows stops
/// drawing the modern frame and paints the classic title bar and border
/// instead, so the overlay drops those styles before it gets a region. The
/// window library adds them back whenever it updates the window (showing it,
/// changing focusability), so this runs before every region change.
#[cfg(windows)]
fn strip_frame_styles(raw: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };
    let hwnd = hwnd_of(raw);
    let frame =
        (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0 as isize;
    // SAFETY: plain Win32 calls on the overlay's own window handle.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let stripped = (style & !frame) | WS_POPUP.0 as isize;
        if stripped != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, stripped);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Lets the overlay take keyboard focus (review) or not (listening, so the app
/// being dictated into keeps it). The window library's `set_focusable`
/// rewrites every window style and redraws the frame, which flashed the
/// classic Windows border around the clipped card for a frame; only the
/// "no activate" flag is flipped here. The library still sees the overlay as
/// not focusable, so its next style update (showing it) puts the flag back,
/// which is also what listening needs.
#[cfg(windows)]
fn set_overlay_activatable(app: &AppHandle, _w: &WebviewWindow, activatable: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    let raw = overlay_hwnd(app);
    if raw == 0 {
        return;
    }
    let hwnd = hwnd_of(raw);
    // SAFETY: plain Win32 calls on the overlay's own window handle.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let flag = WS_EX_NOACTIVATE.0 as isize;
        let next = if activatable { ex & !flag } else { ex | flag };
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
}

#[cfg(not(windows))]
fn set_overlay_activatable(_app: &AppHandle, w: &WebviewWindow, activatable: bool) {
    let _ = w.set_focusable(activatable);
}

/// Clips the overlay window to the card plus its shadow margin, and the open
/// language menu plus its shadow and destination-app tab, each with rounded corners following the
/// element's own (`clip`); or removes the clip while the overlay is hidden.
/// Nothing else of the window is drawn or clickable: its transparent room
/// would otherwise show as a dim box and block clicks to the app behind.
#[cfg(windows)]
fn set_overlay_region(app: &AppHandle, w: &WebviewWindow, clip: bool) {
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_OR,
    };
    let raw = overlay_hwnd(app);
    let Ok(size) = w.outer_size() else { return };
    if raw == 0 {
        return;
    }
    if clip {
        strip_frame_styles(raw);
    }
    let scale = w.scale_factor().unwrap_or(1.0);
    let px = |v: f64| (v * scale).round() as i32;
    let room = room_px(w);
    let menu = *MENU_RECT.lock().unwrap_or_else(|p| p.into_inner());
    let tip = *TIP_RECT.lock().unwrap_or_else(|p| p.into_inner());
    // SAFETY: plain Win32 calls on the overlay's own window handle. The menu
    // region is merged into the card's and freed; on success the system owns
    // the combined region, so it is not freed here.
    unsafe {
        let region = clip.then(|| {
            // CreateRoundRectRgn takes the corner ellipse's size (2x radius) and
            // leaves out the right and bottom edges, hence +1.
            let d = px(CARD_RADIUS + CARD_MARGIN) * 2;
            let card = CreateRoundRectRgn(
                0,
                room,
                size.width as i32 + 1,
                size.height as i32 - room + 1,
                d,
                d,
            );
            let tab = CreateRoundRectRgn(
                px(APP_TAB_WIDTH_INSET),
                room + px(CARD_MARGIN + APP_TAB_OVERLAP - APP_TAB_HEIGHT),
                size.width as i32 - px(APP_TAB_WIDTH_INSET) + 1,
                room + px(CARD_MARGIN - APP_TAB_OVERLAP) + 1,
                px(10.0) * 2,
                px(10.0) * 2,
            );
            // The small card has no destination-app tab above it.
            if !MINI.load(Ordering::Relaxed) {
                CombineRgn(Some(card), Some(card), Some(tab), RGN_OR);
            }
            let _ = DeleteObject(HGDIOBJ(tab.0));
            for [x, y, mw, mh] in [menu, tip].into_iter().flatten() {
                let s = MENU_SHADOW;
                let d = px(MENU_RADIUS + s) * 2;
                let rgn = CreateRoundRectRgn(
                    px(x - s),
                    px(y - s),
                    px(x + mw + s) + 1,
                    px(y + mh + s) + 1,
                    d,
                    d,
                );
                CombineRgn(Some(card), Some(card), Some(rgn), RGN_OR);
                let _ = DeleteObject(HGDIOBJ(rgn.0));
            }
            card
        });
        let _ = SetWindowRgn(hwnd_of(raw), region, true);
    }
}

#[cfg(not(windows))]
fn set_overlay_region(_app: &AppHandle, _w: &WebviewWindow, _clip: bool) {}

/// Runs an overlay transition in one go on the main thread. Window changes
/// requested from another thread (the dictation hotkey, the dictation flow)
/// only run later on the main thread; mixed with the region changes made right
/// away, the window was briefly shown unclipped or with its frame styles back,
/// which flashes a classic Windows border around the card. On the main thread
/// the whole transition happens before the window is painted again (window
/// calls made there run at once instead of being queued).
fn on_main(app: &AppHandle, f: impl FnOnce() + Send + 'static) {
    // Fails only while the app is shutting down; the overlay no longer matters then.
    let _ = app.run_on_main_thread(f);
}

/// Adds the open language menu (`Some(logical x, y, w, h)` in the window) to
/// the visible part of the overlay, or takes it away again (`None`).
pub fn set_overlay_menu(app: &AppHandle, menu: Option<[f64; 4]>) {
    *MENU_RECT.lock().unwrap_or_else(|p| p.into_inner()) = menu;
    if let Some(w) = app.get_webview_window(OVERLAY) {
        let app2 = app.clone();
        on_main(app, move || set_overlay_region(&app2, &w, true));
    }
}

/// Adds a tooltip's rectangle to the visible part of the overlay, or takes it
/// away again (`None`).
pub fn set_overlay_tip(app: &AppHandle, tip: Option<[f64; 4]>) {
    *TIP_RECT.lock().unwrap_or_else(|p| p.into_inner()) = tip;
    if let Some(w) = app.get_webview_window(OVERLAY) {
        let app2 = app.clone();
        on_main(app, move || {
            if w.is_visible().unwrap_or(false) {
                set_overlay_region(&app2, &w, true);
            }
        });
    }
}

/// Switches the overlay between listening (it never takes focus, so the app
/// being dictated into keeps it) and review (focusable: the result is edited).
pub fn set_overlay_review(app: &AppHandle, review: bool) {
    let Ok(w) = create_overlay(app) else { return };
    let app2 = app.clone();
    on_main(app, move || overlay_review_now(&app2, &w, review));
}

fn overlay_review_now(app: &AppHandle, w: &WebviewWindow, review: bool) {
    suppress_persistence();
    // The small layout has no language menu; its editor above the pill lives
    // in this rectangle and must survive the switch.
    if !MINI.load(Ordering::Relaxed) {
        *MENU_RECT.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    set_overlay_activatable(app, w, review);
    // The open language menu (if any) was dropped from the visible area above.
    set_overlay_region(app, w, true);
    if review {
        focus_hwnd(overlay_hwnd(app));
        let _ = w.set_focus();
    }
}

/// Drops a result that is waiting for review and hands focus back. Returns
/// whether there was one.
pub fn discard_review(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let had = state
        .dictation_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .is_some();
    if had {
        set_overlay_review(app, false);
        restore_target(app);
        state.dictation_busy.store(false, Ordering::Relaxed);
    }
    had
}

pub fn create_overlay(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(OVERLAY) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::App("index.html#/overlay".into()))
        .title("Dictation overlay")
        .inner_size(OVERLAY_W, OVERLAY_H + 2.0 * MENU_ROOM)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    // Suppress before attaching the listener: window construction itself can
    // fire a Moved event at some OS-default position, which must not get
    // persisted as if the user had dragged it there.
    suppress_persistence();
    let h = app.clone();
    let w2 = w.clone();
    w.on_window_event(move |e| {
        if let tauri::WindowEvent::Moved(pos) = e {
            if now_ms() < SUPPRESS_UNTIL.load(Ordering::Relaxed) {
                return;
            }
            // Saved as the card's spot, not the window's (which includes the room above).
            let (x, y) = (pos.x, pos.y + room_px(&w2));
            let _ = h.state::<AppState>().settings.update(|s| {
                s.overlay_x = Some(x);
                s.overlay_y = Some(y);
            });
        }
    });
    Ok(w)
}

/// Top-left of the card for the overlay's default centered-near-the-bottom
/// placement.
fn default_overlay_position(w: &WebviewWindow) -> Option<(i32, i32)> {
    let m = w.primary_monitor().ok().flatten()?;
    let area = m.work_area();
    let scale = m.scale_factor();
    let (lw, lh) = card_dims();
    let (cw, ch) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let x = area.position.x + (area.size.width as i32 - cw) / 2;
    // The visible card sits CARD_MARGIN inside its box; leave only a small gap
    // between it and the taskbar.
    let y = area.position.y + area.size.height as i32 - ch
        + ((CARD_MARGIN - OVERLAY_BOTTOM_GAP) * scale).round() as i32;
    Some((x, y))
}

/// Shows the dictation overlay at its last dragged spot, or centered near the
/// bottom without taking focus if it was never dragged (or that spot's
/// monitor is no longer connected).
pub fn show_overlay(app: &AppHandle) {
    let Ok(w) = create_overlay(app) else { return };
    let app2 = app.clone();
    on_main(app, move || show_overlay_now(&app2, &w));
}

/// Keeps a saved card spot fully inside its monitor's work area (the card may
/// have been dragged there when it was smaller, or on another resolution).
fn card_on_screen(w: &WebviewWindow, x: i32, y: i32) -> (i32, i32) {
    let Ok(monitors) = w.available_monitors() else {
        return (x, y);
    };
    let Some(m) = monitors.iter().find(|m| {
        let (p, s) = (m.position(), m.size());
        x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
    }) else {
        return (x, y);
    };
    let scale = m.scale_factor();
    let (lw, lh) = card_dims();
    let size = ((lw * scale).round() as u32, (lh * scale).round() as u32);
    let area = m.work_area();
    clamp_into(
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
        (x, y),
        size,
    )
}

fn show_overlay_now(app: &AppHandle, w: &WebviewWindow) {
    let saved = app.state::<AppState>().settings.get();
    suppress_persistence();
    *MENU_RECT.lock().unwrap_or_else(|p| p.into_inner()) = None;
    *TIP_RECT.lock().unwrap_or_else(|p| p.into_inner()) = None;
    MINI_WIDTH.store(saved.overlay_mini_width, Ordering::Relaxed);
    MINI.store(wants_mini(app), Ordering::Relaxed);
    let _ = w.set_size(overlay_window_size());
    let pos = match (saved.overlay_x, saved.overlay_y) {
        (Some(x), Some(y)) if on_any_monitor(w, x, y) => Some(card_on_screen(w, x, y)),
        _ => default_overlay_position(w),
    };
    if let Some((x, y)) = pos {
        let _ = w.set_position(PhysicalPosition::new(x, y - room_px(w)));
    }
    let _ = w.show();
    // After showing: that re-adds the frame styles the region needs gone.
    set_overlay_region(app, w, true);
}

/// Clears the dragged overlay position and moves it back to the default spot
/// immediately (even while hidden).
pub fn reset_overlay_position(app: &AppHandle) {
    let _ = app.state::<AppState>().settings.update(|s| {
        s.overlay_x = None;
        s.overlay_y = None;
    });
    let Ok(w) = create_overlay(app) else { return };
    suppress_persistence();
    if let Some((x, y)) = default_overlay_position(&w) {
        let _ = w.set_position(PhysicalPosition::new(x, y - room_px(&w)));
    }
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(OVERLAY) {
        let app2 = app.clone();
        on_main(app, move || {
            hide_overlay_window(&app2, &w);
        });
    }
}

/// Applies a pin change: a pinned overlay appears now (idle), an unpinned idle
/// one goes away. A running dictation is left alone.
pub fn sync_overlay_pin(app: &AppHandle) {
    let Ok(w) = create_overlay(app) else { return };
    let app2 = app.clone();
    on_main(app, move || {
        let state = app2.state::<AppState>();
        if state.voice.is_active() || state.dictation_busy.load(Ordering::Relaxed) {
            return;
        }
        // Pinned: docks and shows. Unpinned: really hides.
        hide_overlay_window(&app2, &w);
    });
}

/// Opens the overlay as its idle card (tray entry): nothing is recorded until
/// the shortcut or the mic button is used. A running dictation is left alone.
pub fn show_idle_overlay(app: &AppHandle) {
    let Ok(w) = create_overlay(app) else { return };
    let app2 = app.clone();
    on_main(app, move || {
        let state = app2.state::<AppState>();
        if state.voice.is_active() || state.dictation_busy.load(Ordering::Relaxed) {
            return;
        }
        show_overlay_now(&app2, &w);
        let _ = app2.emit_to(OVERLAY, "overlay://idle", ());
    });
}

/// Sets the small overlay's width right away (a drag on its edge). Only the
/// window changes here; the command saves it when the drag ends.
pub fn resize_overlay_mini(app: &AppHandle, width: u32) {
    let Some(w) = app.get_webview_window(OVERLAY) else {
        return;
    };
    let app2 = app.clone();
    on_main(app, move || {
        if !MINI.load(Ordering::Relaxed) {
            return;
        }
        suppress_persistence();
        MINI_WIDTH.store(width, Ordering::Relaxed);
        let _ = w.set_size(overlay_window_size());
        set_overlay_region(&app2, &w, true);
    });
}

/// Re-applies the small/full layout after the mini setting changed.
pub fn refresh_overlay_layout(app: &AppHandle) {
    let Some(w) = app.get_webview_window(OVERLAY) else {
        return;
    };
    let app2 = app.clone();
    on_main(app, move || {
        if w.is_visible().unwrap_or(false) {
            show_overlay_now(&app2, &w);
        }
    });
}

/// Hides only if the delayed dismissal still belongs to the current, idle
/// dictation session. The check runs on the UI thread with the hide itself, so
/// an old timer cannot hide an overlay a newer session has reopened.
pub fn hide_overlay_if_session_idle(app: &AppHandle, generation: u64) {
    if let Some(w) = app.get_webview_window(OVERLAY) {
        let app2 = app.clone();
        on_main(app, move || {
            let state = app2.state::<AppState>();
            if !overlay_hide_is_current_and_idle(
                generation,
                state.dictation_session_generation.load(Ordering::Relaxed),
                state.voice.is_active(),
                state.dictation_busy.load(Ordering::Relaxed),
            ) {
                return;
            }
            hide_overlay_window(&app2, &w);
        });
    }
}

fn overlay_hide_is_current_and_idle(
    scheduled_generation: u64,
    current_generation: u64,
    dictation_active: bool,
    busy: bool,
) -> bool {
    scheduled_generation == current_generation && !dictation_active && !busy
}

/// Pinned overlays never hide: they fall back to their idle card (the page
/// resets itself on `overlay://idle`). Nothing is listened to while idle.
fn hide_overlay_window(app: &AppHandle, w: &WebviewWindow) {
    if app.state::<AppState>().settings.get().overlay_pinned {
        show_overlay_now(app, w);
        let _ = app.emit_to(OVERLAY, "overlay://idle", ());
        return;
    }
    let _ = w.hide();
    // Showing it again changes its styles; that must not happen while clipped.
    set_overlay_region(app, w, false);
}

/// Closes the overlay right away (Esc / overlay X). Whatever is in flight is
/// dropped: listening stops without inserting, a result waiting for review is
/// discarded, and a transcript still being transcribed or corrected is
/// cancelled when it arrives. Closing an overlay that only shows a finished
/// result ("Inserted", an error) just hides it.
pub fn cancel_dictation(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !discard_review(app) {
        // Also covers the transcribing/correcting stretch, when nothing is
        // listening yet run_dictation still has to see the cancel. A stale flag
        // is harmless: the next session clears it on start.
        state.dictation_cancel.store(true, Ordering::Relaxed);
        if state.voice.is_active() {
            state.voice.stop(true);
        }
    }
    hide_overlay(app);
}
