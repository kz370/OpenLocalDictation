//! Optional mic button inside the Windows taskbar (specs/plans/taskbar-widget.md).
//!
//! The button is a small native window made a child of the taskbar window, so
//! the taskbar never covers it. It lives on its own thread with its own message
//! loop and does nothing but draw and forward input: a child of another
//! process's window shares that process's input queue, so a stalled widget
//! thread would stall the taskbar. Everything slow happens in the app, which
//! only talks to this module through `PostMessageW` (state changes) and the
//! callbacks in [`Config`] (input).
//!
//! Only a bottom taskbar is supported. On other targets the module is a stub.

use serde::Serialize;
use std::sync::{Arc, Mutex};

/// What the button shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetState {
    /// Mic in the accent color.
    Idle,
    /// Red stop square with a pulsing ring.
    Listening,
    /// Spinner: transcribing or correcting.
    Busy,
    /// Pencil: a result waits for review.
    Review,
    /// Green check: inserted.
    Done,
    /// Grey cross: nothing heard, cancelled or failed.
    Fail,
}

impl WidgetState {
    /// Maps a `dictation://state` name to what the button shows.
    pub fn from_dictation(name: &str) -> Option<Self> {
        Some(match name {
            "idle" => Self::Idle,
            "listening" => Self::Listening,
            "transcribing" | "correcting" | "reviewing" => Self::Busy,
            "review" => Self::Review,
            "inserted" => Self::Done,
            "empty" | "cancelled" | "error" => Self::Fail,
            _ => return None,
        })
    }
}

/// Input forwarded from the widget thread to the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Clicked,
    /// The pointer rested on the button for the hover delay.
    HoverStart,
    HoverEnd,
    /// A language (`auto` or a code) was chosen from the menu a right click on
    /// the button opens above it.
    LanguagePicked(String),
    /// The button has a position to remember: where a drag left it, or the
    /// default spot it was given the first time. Logical px from the taskbar's
    /// left edge.
    Moved(i32),
}

type EventFn = dyn Fn(Event) + Send + Sync;
type StatusFn = dyn Fn(Status) + Send + Sync;

/// Everything the widget needs from the app. The callbacks run on the widget
/// thread and must return at once.
#[derive(Clone)]
pub struct Config {
    /// Saved position (logical px from the taskbar's left edge); `None` until
    /// it has been placed once.
    pub x: Option<i32>,
    /// The dictation language setting (`auto` or a code), shown beside the mic.
    pub language: String,
    pub on_event: Arc<EventFn>,
    pub on_status: Arc<StatusFn>,
}

/// One entry of the language menu.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct LangItem {
    pub code: String,
    pub label: String,
}

static LANGS: Mutex<Vec<LangItem>> = Mutex::new(Vec::new());

/// The color profile's index in `profile::PROFILES`: the button's accent.
static PROFILE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The languages the menu offers (the overlay page knows their names and which
/// ones are shown, and sends them whenever they change).
pub fn set_languages(items: Vec<LangItem>) {
    *LANGS.lock().unwrap_or_else(|p| p.into_inner()) = items;
}

fn languages() -> Vec<LangItem> {
    LANGS.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// What the language indicator says: `AUTO`, or the code in capitals.
pub fn language_label(code: &str) -> String {
    let code = code.trim();
    if code.is_empty() || code.eq_ignore_ascii_case("auto") {
        return "AUTO".into();
    }
    code.chars().take(4).flat_map(char::to_uppercase).collect()
}

/// Whether the button is on the taskbar, and if not, why.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Status {
    pub shown: bool,
    pub reason: Option<String>,
}

/// Where the button is, in physical px of the primary monitor, for placing the
/// pill above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub center_x: i32,
    pub center_y: i32,
    /// Top edge of the taskbar.
    pub top: i32,
    /// DPI scale of the taskbar's monitor (1.0 = 96 dpi).
    pub scale: f64,
}

struct Shared {
    status: Status,
    anchor: Option<Anchor>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    status: Status {
        shown: false,
        reason: None,
    },
    anchor: None,
});

#[cfg(test)]
mod label_tests {
    use super::language_label;

    #[test]
    fn the_indicator_says_auto_or_the_code_in_capitals() {
        assert_eq!(language_label("auto"), "AUTO");
        assert_eq!(language_label(""), "AUTO");
        assert_eq!(language_label("en"), "EN");
        assert_eq!(language_label("ar"), "AR");
        assert_eq!(language_label("zh-cn-long"), "ZH-C");
    }
}

pub fn status() -> Status {
    SHARED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .status
        .clone()
}

/// The button's position while it is on the taskbar.
pub fn anchor() -> Option<Anchor> {
    SHARED.lock().unwrap_or_else(|p| p.into_inner()).anchor
}

#[cfg(windows)]
pub use imp::{set_language, set_mic_ok, set_profile, set_state, set_x, start, stop};

#[cfg(not(windows))]
pub fn start(_config: Config) {}
#[cfg(not(windows))]
pub fn stop() {}
#[cfg(not(windows))]
pub fn set_state(_state: WidgetState) {}
#[cfg(not(windows))]
pub fn set_x(_x: Option<i32>) {}
#[cfg(not(windows))]
pub fn set_mic_ok(_ok: bool) {}
#[cfg(not(windows))]
pub fn set_language(_code: &str) {}
#[cfg(not(windows))]
pub fn set_profile(_id: &str) {}

/// Drawing: pure software rendering into premultiplied BGRA, so the button has
/// per-pixel alpha (soft edges, halo) without a graphics library.
mod draw {
    use super::WidgetState;

    #[derive(Clone, Copy)]
    struct Rgb(f32, f32, f32);

    const fn hex(c: u32) -> Rgb {
        Rgb(
            ((c >> 16) & 0xff) as f32 / 255.0,
            ((c >> 8) & 0xff) as f32 / 255.0,
            (c & 0xff) as f32 / 255.0,
        )
    }

    /// Logical height of the button's window, and the side of the square its
    /// disc sits in.
    pub const BOX: f32 = 32.0;
    /// Logical width of the language indicator to the right of the disc.
    pub const CHIP_W: f32 = 50.0;
    const R: f32 = 11.5;
    /// Half the height of the capsule that holds the disc and the indicator.
    const CAPSULE_HALF_H: f32 = 14.5;

    /// Premultiplied pixel.
    type Px = [f32; 4];

    /// Coverage (0..1) of rendered text, one value per physical pixel.
    pub struct Mask {
        pub w: usize,
        pub h: usize,
        pub cov: Vec<f32>,
    }

    struct Canvas {
        w: usize,
        h: usize,
        scale: f32,
        px: Vec<Px>,
    }

    impl Canvas {
        /// Composites `color` at `alpha` where `sd(x, y)` (signed distance in
        /// logical px from the disc's center, negative inside) says so.
        fn paint(&mut self, color: Rgb, alpha: f32, sd: impl Fn(f32, f32) -> f32) {
            let half = self.h as f32 / 2.0;
            for y in 0..self.h {
                for x in 0..self.w {
                    let lx = (x as f32 + 0.5 - half) / self.scale;
                    let ly = (y as f32 + 0.5 - half) / self.scale;
                    let cov = (0.5 - sd(lx, ly) * self.scale).clamp(0.0, 1.0) * alpha;
                    if cov <= 0.0 {
                        continue;
                    }
                    let d = &mut self.px[y * self.w + x];
                    let keep = 1.0 - cov;
                    d[0] = color.0 * cov + d[0] * keep;
                    d[1] = color.1 * cov + d[1] * keep;
                    d[2] = color.2 * cov + d[2] * keep;
                    d[3] = cov + d[3] * keep;
                }
            }
        }
    }

    impl Canvas {
        /// Composites text coverage at physical column `x0`.
        fn blit_mask(&mut self, x0: usize, mask: &Mask, color: Rgb) {
            for y in 0..mask.h.min(self.h) {
                for x in 0..mask.w {
                    let Some(&cov) = mask.cov.get(y * mask.w + x) else {
                        continue;
                    };
                    if cov <= 0.0 || x0 + x >= self.w {
                        continue;
                    }
                    let d = &mut self.px[y * self.w + x0 + x];
                    let keep = 1.0 - cov;
                    d[0] = color.0 * cov + d[0] * keep;
                    d[1] = color.1 * cov + d[1] * keep;
                    d[2] = color.2 * cov + d[2] * keep;
                    d[3] = cov + d[3] * keep;
                }
            }
        }

        /// Like `paint`, with a color that runs from `top` (at -span) to
        /// `bottom` (at +span) down the box.
        fn paint_vertical(
            &mut self,
            top: Rgb,
            bottom: Rgb,
            span: f32,
            sd: impl Fn(f32, f32) -> f32,
        ) {
            let half = self.h as f32 / 2.0;
            for y in 0..self.h {
                for x in 0..self.w {
                    let lx = (x as f32 + 0.5 - half) / self.scale;
                    let ly = (y as f32 + 0.5 - half) / self.scale;
                    let cov = (0.5 - sd(lx, ly) * self.scale).clamp(0.0, 1.0);
                    if cov <= 0.0 {
                        continue;
                    }
                    let k = ((ly + span) / (2.0 * span)).clamp(0.0, 1.0);
                    let mix = |a: f32, b: f32| a + (b - a) * k;
                    let d = &mut self.px[y * self.w + x];
                    let keep = 1.0 - cov;
                    d[0] = mix(top.0, bottom.0) * cov + d[0] * keep;
                    d[1] = mix(top.1, bottom.1) * cov + d[1] * keep;
                    d[2] = mix(top.2, bottom.2) * cov + d[2] * keep;
                    d[3] = cov + d[3] * keep;
                }
            }
        }
    }

    fn circle(x: f32, y: f32, r: f32) -> f32 {
        x.hypot(y) - r
    }

    fn ring(x: f32, y: f32, r: f32, thick: f32) -> f32 {
        (x.hypot(y) - r).abs() - thick / 2.0
    }

    fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32), r: f32) -> f32 {
        let (px, py) = (x - a.0, y - a.1);
        let (bx, by) = (b.0 - a.0, b.1 - a.1);
        let len2 = bx * bx + by * by;
        let h = if len2 == 0.0 {
            0.0
        } else {
            ((px * bx + py * by) / len2).clamp(0.0, 1.0)
        };
        (px - bx * h).hypot(py - by * h) - r
    }

    fn round_rect(x: f32, y: f32, half: f32, r: f32) -> f32 {
        let (qx, qy) = (x.abs() - (half - r), y.abs() - (half - r));
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    }

    /// Palette of the app's tokens (tokens.css), light and dark.
    struct Palette {
        accent: Rgb,
        accent_ink: Rgb,
        danger: Rgb,
        danger_ink: Rgb,
        success: Rgb,
        success_ink: Rgb,
        muted: Rgb,
        muted_ink: Rgb,
    }

    fn palette(light_taskbar: bool) -> Palette {
        let profile =
            &crate::profile::PROFILES[super::PROFILE.load(std::sync::atomic::Ordering::Relaxed)];
        if light_taskbar {
            Palette {
                accent: hex(profile.accent_light),
                accent_ink: hex(profile.ink_light),
                danger: hex(0xb42318),
                danger_ink: hex(0xffffff),
                success: hex(0x067647),
                success_ink: hex(0xffffff),
                muted: hex(0x6b7280),
                muted_ink: hex(0xffffff),
            }
        } else {
            Palette {
                accent: hex(profile.accent_dark),
                accent_ink: hex(profile.ink_dark),
                danger: hex(0xf97066),
                danger_ink: hex(0x3a1d1b),
                success: hex(0x47cd89),
                success_ink: hex(0x16301f),
                muted: hex(0x9ca3af),
                muted_ink: hex(0x1f2937),
            }
        }
    }

    fn lighten(c: Rgb, by: f32) -> Rgb {
        Rgb(
            c.0 + (1.0 - c.0) * by,
            c.1 + (1.0 - c.1) * by,
            c.2 + (1.0 - c.2) * by,
        )
    }

    /// Height of the window in physical px.
    pub fn height_px(scale: f32) -> usize {
        (BOX * scale).round().max(8.0) as usize
    }

    /// Width of the window in physical px.
    pub fn width_px(scale: f32) -> usize {
        ((BOX + CHIP_W) * scale).round().max(8.0) as usize
    }

    /// Physical px from the window's left edge to where the indicator starts
    /// (just right of the disc).
    pub fn chip_left_px(scale: f32) -> usize {
        ((BOX - 3.0) * scale).round() as usize
    }

    /// One frame's inputs.
    pub struct Frame<'a> {
        pub state: WidgetState,
        pub hover: bool,
        pub pressed: bool,
        pub light_taskbar: bool,
        pub mic_ok: bool,
        pub scale: f32,
        /// Drives the animations.
        pub t_ms: u64,
        /// The language indicator's text, as coverage over the chip area.
        pub label: Option<&'a Mask>,
    }

    /// Signed distance to a rounded box centered at `(cx, 0)`.
    fn capsule(x: f32, y: f32, cx: f32, half_w: f32, half_h: f32) -> f32 {
        let r = half_h;
        let (qx, qy) = ((x - cx).abs() - (half_w - r), y.abs() - (half_h - r));
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
    }

    /// Renders one frame: (width, height, premultiplied BGRA pixels).
    pub fn render(f: &Frame) -> (usize, usize, Vec<u32>) {
        let Frame {
            state,
            hover,
            pressed,
            light_taskbar,
            mic_ok,
            scale,
            t_ms,
            label,
        } = *f;
        let (w, h) = (width_px(scale), height_px(scale));
        let mut c = Canvas {
            w,
            h,
            scale,
            px: vec![[0.0; 4]; w * h],
        };
        let p = palette(light_taskbar);
        // No usable microphone: the idle button is red, with a slashed mic.
        let alert = state == WidgetState::Idle && !mic_ok;
        let (base, ink) = match state {
            _ if alert => (p.danger, p.danger_ink),
            WidgetState::Listening => (p.danger, p.danger_ink),
            WidgetState::Done => (p.success, p.success_ink),
            WidgetState::Fail => (p.muted, p.muted_ink),
            _ => (p.accent, p.accent_ink),
        };
        let disc = if hover || pressed {
            lighten(base, if pressed { 0.05 } else { 0.14 })
        } else {
            base
        };
        let t = t_ms as f32;

        // A capsule holds the disc and the language indicator; it brightens
        // when pointed at. Inside it, a flat disc: no halo at rest (it read
        // as a smudge on the taskbar), a pulse while listening.
        let tint = if light_taskbar {
            Rgb(0.0, 0.0, 0.0)
        } else {
            Rgb(1.0, 1.0, 1.0)
        };
        // Capsule edges in disc-center coordinates: 3 px clear of the disc on
        // the left, half a px inside the window on the right.
        let (left, right) = (-(R + 3.0), BOX + CHIP_W - BOX / 2.0 - 0.5);
        let (cx, half_w) = ((left + right) / 2.0, (right - left) / 2.0);
        let lit = hover || pressed;
        let fill = match (light_taskbar, lit) {
            (true, false) => 0.07,
            (true, true) => 0.13,
            (false, false) => 0.10,
            (false, true) => 0.17,
        };
        c.paint(tint, fill, |x, y| capsule(x, y, cx, half_w, CAPSULE_HALF_H));
        c.paint(tint, 0.14, |x, y| {
            capsule(x, y, cx, half_w, CAPSULE_HALF_H).abs() - 0.5
        });
        if state == WidgetState::Listening {
            let phase = (t % 1400.0) / 1400.0;
            c.paint(base, (1.0 - phase) * 0.7, |x, y| {
                ring(x, y, R + 0.6 + phase * 2.4, 1.3)
            });
        }
        // Soft top-to-bottom shading, and a hairline edge for definition.
        c.paint_vertical(lighten(disc, 0.14), disc, R, |x, y| circle(x, y, R));
        c.paint(Rgb(0.0, 0.0, 0.0), 0.16, |x, y| ring(x, y, R - 0.5, 1.0));

        match state {
            WidgetState::Idle => {
                mic(&mut c, ink);
                if alert {
                    c.paint(ink, 1.0, |x, y| {
                        segment(x, y, (-6.0, -6.5), (6.0, 6.5), 1.0)
                    });
                }
            }
            WidgetState::Listening => c.paint(ink, 1.0, |x, y| round_rect(x, y, 4.6, 1.4)),
            WidgetState::Busy => {
                let rot = t / 800.0 * std::f32::consts::TAU;
                let sweep = 1.5 * std::f32::consts::PI;
                let arc = move |x: f32, y: f32| {
                    let along = (y.atan2(x) - rot).rem_euclid(std::f32::consts::TAU);
                    if along < sweep {
                        ring(x, y, 6.5, 2.2)
                    } else {
                        1.0
                    }
                };
                c.paint(ink, 1.0, arc);
            }
            WidgetState::Review => {
                c.paint(ink, 1.0, |x, y| {
                    segment(x, y, (-4.0, 3.5), (3.5, -4.0), 1.8)
                });
                c.paint(ink, 1.0, |x, y| {
                    segment(x, y, (-5.5, 6.5), (5.5, 6.5), 0.75)
                });
            }
            WidgetState::Done => {
                c.paint(ink, 1.0, |x, y| {
                    segment(x, y, (-4.5, 0.5), (-1.5, 3.5), 1.15).min(segment(
                        x,
                        y,
                        (-1.5, 3.5),
                        (4.5, -3.5),
                        1.15,
                    ))
                });
            }
            WidgetState::Fail => {
                c.paint(ink, 1.0, |x, y| {
                    segment(x, y, (-4.0, -4.0), (4.0, 4.0), 1.15).min(segment(
                        x,
                        y,
                        (-4.0, 4.0),
                        (4.0, -4.0),
                        1.15,
                    ))
                });
            }
        }

        if let Some(mask) = label {
            let ink = if light_taskbar {
                hex(0x1f2937)
            } else {
                hex(0xe5e7eb)
            };
            c.blit_mask(chip_left_px(scale), mask, ink);
        }

        let out =
            c.px.iter()
                .map(|p| {
                    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                    (q(p[3]) << 24) | (q(p[0]) << 16) | (q(p[1]) << 8) | q(p[2])
                })
                .collect();
        (w, h, out)
    }

    fn mic(c: &mut Canvas, ink: Rgb) {
        // Capsule, the cradle around it, the stem and the base.
        let cy = -1.0;
        c.paint(ink, 1.0, |x, y| segment(x, y, (0.0, -4.3), (0.0, cy), 2.5));
        c.paint(ink, 1.0, |x, y| ring(x, y - cy, 5.0, 1.5).max(-(y - cy)));
        c.paint(ink, 1.0, |x, y| {
            segment(x, y, (0.0, cy + 5.0), (0.0, 7.2), 0.75)
        });
        c.paint(ink, 1.0, |x, y| {
            segment(x, y, (-2.8, 7.2), (2.8, 7.2), 0.75)
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn renders_every_state_inside_the_box() {
            for state in [
                WidgetState::Idle,
                WidgetState::Listening,
                WidgetState::Busy,
                WidgetState::Review,
                WidgetState::Done,
                WidgetState::Fail,
            ] {
                for scale in [1.0, 1.5, 2.0] {
                    let frame = |state, mic_ok| Frame {
                        state,
                        hover: false,
                        pressed: false,
                        light_taskbar: true,
                        mic_ok,
                        scale,
                        t_ms: 300,
                        label: None,
                    };
                    let (w, h, px) = render(&frame(state, true));
                    let (_, _, alert) = render(&frame(WidgetState::Idle, false));
                    assert_ne!(alert, px);
                    assert_eq!(px.len(), w * h);
                    assert!(w > h);
                    // The disc is opaque in the middle and the corners stay clear.
                    assert!(px[w * (h / 2) + h / 2] >> 24 == 0xff);
                    assert_eq!(px[0] >> 24, 0);
                }
            }
        }

        #[test]
        fn the_language_label_is_drawn_inside_the_capsule() {
            let scale = 1.0;
            let (w, h) = (width_px(scale), height_px(scale));
            let mask = Mask {
                w: w - chip_left_px(scale),
                h,
                cov: vec![1.0; (w - chip_left_px(scale)) * h],
            };
            let frame = |label| Frame {
                state: WidgetState::Idle,
                hover: false,
                pressed: false,
                light_taskbar: false,
                mic_ok: true,
                scale,
                t_ms: 0,
                label,
            };
            let (_, _, plain) = render(&frame(None));
            let (_, _, labelled) = render(&frame(Some(&mask)));
            assert_ne!(plain, labelled);
            // Opaque ink in the middle of the indicator, nothing past the window.
            assert_eq!(labelled[w * (h / 2) + w - 10] >> 24, 0xff);
            assert_eq!(labelled.len(), w * h);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{draw, Anchor, Config, Event, Status, WidgetState, SHARED};
    use std::cell::{Cell, RefCell};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU8, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW,
        GdiFlush, GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY,
        BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
        DIB_RGB_COLORS, DT_CENTER, DT_NOCLIP, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HGDIOBJ,
        OUT_DEFAULT_PRECIS, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE,
        TRACKMOUSEEVENT, VK_CONTROL, VK_RBUTTON,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;

    // Messages the app posts to the control window.
    const WM_SET_STATE: u32 = WM_APP + 1;
    const WM_SET_X: u32 = WM_APP + 2;
    const WM_STOP: u32 = WM_APP + 3;
    const WM_MIC: u32 = WM_APP + 4;
    const WM_LANG: u32 = WM_APP + 5;
    const WM_MOUSELEAVE: u32 = 0x02A3;
    /// `wParam` flag of a mouse message: Ctrl was down.
    const MK_CONTROL: usize = 0x0008;

    // Timers (control window).
    const T_POLL: usize = 1;
    const T_IDLE: usize = 2;
    const T_LINGER: usize = 3;
    // Timers (widget window).
    const T_HOVER: usize = 10;
    const T_ANIM: usize = 11;

    /// A press that moves less than this (physical px) is a click, not a drag.
    const DRAG_SLOP: i32 = 4;
    const POLL_MS: u32 = 500;
    const HOVER_MS: u32 = 300;
    /// A right release this soon after a right press closed the menu belongs
    /// to that press (it does not open the menu again).
    const TOGGLE_MS: u64 = 1500;
    /// Idle is held back this long: the voice session reports "idle" a moment
    /// before the dictation that follows reports its own state.
    const IDLE_DELAY_MS: u32 = 200;
    const RETRY_EVERY: Duration = Duration::from_secs(5);
    /// Gap between the button and what sits next to it (logical px).
    const GAP: f32 = 4.0;

    const SIDE_REASON: &str =
        "The taskbar is not at the bottom of the screen; the button only fits a bottom taskbar.";
    const NOT_FOUND_REASON: &str = "The taskbar was not found (Explorer may be restarting).";
    const UNRECOGNISED_REASON: &str =
        "The taskbar is not recognised (a taskbar replacement?); the button cannot be placed.";

    static CTL: AtomicIsize = AtomicIsize::new(0);
    static MIC_OK: AtomicBool = AtomicBool::new(true);
    static MENU_OPEN: AtomicBool = AtomicBool::new(false);
    /// When the language menu was last closed by a right press on the button
    /// (see `language_menu`).
    static MENU_CLOSED_AT: Mutex<Option<Instant>> = Mutex::new(None);
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static WANTED: AtomicBool = AtomicBool::new(false);
    static STATE: AtomicU8 = AtomicU8::new(0);
    /// The button's position, logical px from the taskbar's left edge; -1 until
    /// it has one. Fixed once set: nothing else on the taskbar moves it.
    static X: AtomicI32 = AtomicI32::new(-1);
    static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
    /// The language indicator's text (`AUTO`, `EN`, ...).
    static LABEL: Mutex<String> = Mutex::new(String::new());
    /// The language setting itself (`auto` or a code): the menu ticks it.
    static LANGUAGE: Mutex<String> = Mutex::new(String::new());
    /// The foreground window just before the last click on the button.
    static PREV_FG: AtomicIsize = AtomicIsize::new(0);

    /// Gives the focus back to the window that had it before the click on the
    /// button, so the app being dictated into keeps it.
    /// Notes the window that has the focus unless it is part of the shell.
    fn note_foreground() {
        // SAFETY: plain queries.
        let fg = unsafe { GetForegroundWindow() };
        if fg.0.is_null() {
            return;
        }
        let class = class_of(fg);
        let shell = class.starts_with("Shell_")
            || class.starts_with("OpenLocalDictationTaskbar")
            || matches!(
                class.as_str(),
                "Windows.UI.Core.CoreWindow"
                    | "TopLevelWindowForOverflowXamlIsland"
                    | "XamlExplorerHostIslandWindow"
                    | "NotifyIconOverflowWindow"
            );
        if !shell {
            PREV_FG.store(raw(fg), Ordering::SeqCst);
        }
    }

    fn restore_foreground() {
        let prev = PREV_FG.load(Ordering::SeqCst);
        if prev == 0 {
            return;
        }
        let prev = hwnd(prev);
        // SAFETY: plain queries; the handle is checked before it is used.
        unsafe {
            if IsWindow(Some(prev)).as_bool() && GetForegroundWindow() != prev {
                let _ = SetForegroundWindow(prev);
            }
        }
    }

    fn hwnd(raw: isize) -> HWND {
        HWND(raw as *mut c_void)
    }

    fn raw(h: HWND) -> isize {
        h.0 as isize
    }

    fn state_code(s: WidgetState) -> u8 {
        s as u8
    }

    fn state_from_code(c: u8) -> WidgetState {
        match c {
            1 => WidgetState::Listening,
            2 => WidgetState::Busy,
            3 => WidgetState::Review,
            4 => WidgetState::Done,
            5 => WidgetState::Fail,
            _ => WidgetState::Idle,
        }
    }

    /// State owned by the widget thread. Cells only: window procedures re-enter
    /// each other (creating a window sends messages), so no borrows are held.
    struct Local {
        widget: Cell<isize>,
        shown: Cell<WidgetState>,
        hover: Cell<bool>,
        tracking: Cell<bool>,
        hover_fired: Cell<bool>,
        /// The pointer is over the language indicator, not the mic disc.
        on_chip: Cell<bool>,
        pressed: Cell<bool>,
        drag_armed: Cell<bool>,
        dragging: Cell<bool>,
        drag_x: Cell<i32>,
        drag_offset: Cell<i32>,
        light: Cell<bool>,
        scale: Cell<f32>,
        started: Cell<Option<Instant>>,
        last_try: Cell<Option<Instant>>,
        warned: Cell<bool>,
        created_msg: Cell<u32>,
        placed: Cell<Option<Placed>>,
        /// The indicator's rendered text: (label, coverage).
        label: RefCell<Option<(String, draw::Mask)>>,
    }

    #[derive(Clone, Copy, PartialEq)]
    struct Placed {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    thread_local! {
        static L: Local = const { Local {
            widget: Cell::new(0),
            shown: Cell::new(WidgetState::Idle),
            hover: Cell::new(false),
            tracking: Cell::new(false),
            hover_fired: Cell::new(false),
            on_chip: Cell::new(false),
            pressed: Cell::new(false),
            drag_armed: Cell::new(false),
            dragging: Cell::new(false),
            drag_x: Cell::new(0),
            drag_offset: Cell::new(0),
            light: Cell::new(false),
            scale: Cell::new(1.0),
            started: Cell::new(None),
            last_try: Cell::new(None),
            warned: Cell::new(false),
            created_msg: Cell::new(0),
            placed: Cell::new(None),
            label: RefCell::new(None),
        } };
    }

    fn emit(ev: Event) {
        let cfg = CONFIG.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(cfg) = cfg {
            (cfg.on_event)(ev);
        }
    }

    fn set_status(shown: bool, reason: Option<&str>) {
        let next = Status {
            shown,
            reason: reason.map(str::to_string),
        };
        let changed = {
            let mut g = SHARED.lock().unwrap_or_else(|p| p.into_inner());
            let changed = g.status != next;
            g.status = next.clone();
            if !shown {
                g.anchor = None;
            }
            changed
        };
        if changed {
            let cfg = CONFIG.lock().unwrap_or_else(|p| p.into_inner()).clone();
            if let Some(cfg) = cfg {
                (cfg.on_status)(next);
            }
        }
    }

    pub fn start(config: Config) {
        X.store(config.x.unwrap_or(-1), Ordering::Relaxed);
        *LABEL.lock().unwrap_or_else(|p| p.into_inner()) = super::language_label(&config.language);
        *LANGUAGE.lock().unwrap_or_else(|p| p.into_inner()) = config.language.clone();
        *CONFIG.lock().unwrap_or_else(|p| p.into_inner()) = Some(config);
        WANTED.store(true, Ordering::SeqCst);
        if RUNNING.swap(true, Ordering::SeqCst) {
            return; // the running thread picks the new config up on its next loop
        }
        let spawned = std::thread::Builder::new()
            .name("taskbar-widget".into())
            .spawn(|| loop {
                run();
                // A stop followed by a start: go round again.
                if WANTED.load(Ordering::SeqCst) {
                    continue;
                }
                RUNNING.store(false, Ordering::SeqCst);
                // A start that raced the exit above must not be lost.
                if WANTED.load(Ordering::SeqCst) && !RUNNING.swap(true, Ordering::SeqCst) {
                    continue;
                }
                break;
            });
        if spawned.is_err() {
            RUNNING.store(false, Ordering::SeqCst);
            tracing::warn!("could not start the taskbar widget thread");
        }
    }

    pub fn stop() {
        WANTED.store(false, Ordering::SeqCst);
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: posting to a window handle is always sound; a stale one just fails.
            unsafe {
                let _ = PostMessageW(Some(hwnd(ctl)), WM_STOP, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn set_state(state: WidgetState) {
        STATE.store(state_code(state), Ordering::Relaxed);
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: as in `stop`.
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd(ctl)),
                    WM_SET_STATE,
                    WPARAM(state_code(state) as usize),
                    LPARAM(0),
                );
            }
        }
    }

    /// Whether a usable microphone exists; while not, the idle button is red.
    pub fn set_mic_ok(ok: bool) {
        MIC_OK.store(ok, Ordering::Relaxed);
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: as in `stop`.
            unsafe {
                let _ = PostMessageW(Some(hwnd(ctl)), WM_MIC, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Sets the color profile (`profile::PROFILES` id): the button's accent.
    /// Red (no microphone, listening) belongs to no profile.
    pub fn set_profile(id: &str) {
        let next = crate::profile::index(id);
        if super::PROFILE.swap(next, Ordering::Relaxed) == next {
            return;
        }
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: as in `stop`.
            unsafe {
                // The same message as a microphone change: a redraw.
                let _ = PostMessageW(Some(hwnd(ctl)), WM_MIC, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Sets the dictation language shown beside the mic (`auto` or a code).
    pub fn set_language(code: &str) {
        *LANGUAGE.lock().unwrap_or_else(|p| p.into_inner()) = code.to_string();
        let label = super::language_label(code);
        {
            let mut g = LABEL.lock().unwrap_or_else(|p| p.into_inner());
            if *g == label {
                return;
            }
            *g = label;
        }
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: as in `stop`.
            unsafe {
                let _ = PostMessageW(Some(hwnd(ctl)), WM_LANG, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Shows the language menu just above the taskbar at the pointer (the
    /// button was right-clicked) and returns the code chosen, if any.
    ///
    /// It runs on the widget's own thread and the widget owns it. The widget
    /// is a child of Explorer's taskbar, so this thread shares its input with
    /// the taskbar, which the right click has just brought to the front: the
    /// menu opens in the foreground without taking it. A menu on another
    /// thread, owned by a window of its own, had to take the foreground from
    /// Explorer and was closed at once. The menu's modal loop keeps dispatching
    /// this thread's messages, so the button keeps drawing while it is open.
    fn language_menu(owner: HWND) -> Option<String> {
        // A right click on the button while the menu is open closes it on the
        // press; the release that follows must not open it again (by then the
        // focus has gone back to the app, and a menu opened then would not
        // close on a click elsewhere). So a second right click toggles it shut.
        let closed_on_button = MENU_CLOSED_AT
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
            .is_some_and(|t| t.elapsed() < Duration::from_millis(TOGGLE_MS));
        if closed_on_button {
            return None;
        }
        // One menu at a time.
        if MENU_OPEN.swap(true, Ordering::SeqCst) {
            return None;
        }
        let picked = language_menu_inner(owner);
        MENU_OPEN.store(false, Ordering::SeqCst);
        if picked.is_none() && over_window(owner) && right_button_down() {
            *MENU_CLOSED_AT.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
        }
        picked
    }

    /// Whether the pointer is over `h`.
    fn over_window(h: HWND) -> bool {
        // SAFETY: a plain query.
        unsafe { WindowFromPoint(cursor()) == h }
    }

    /// Whether the right mouse button is held right now.
    fn right_button_down() -> bool {
        // SAFETY: a plain query.
        unsafe { GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0 }
    }

    /// Called on this thread (out of context, through the menu's modal loop)
    /// when another window comes to the front while the language menu is
    /// open: the menu closes, as a menu does when its app loses the front.
    unsafe extern "system" fn close_menu_on_foreground(
        _hook: HWINEVENTHOOK,
        _event: u32,
        fg: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        // The taskbar itself, or the menu, coming to the front is not a
        // reason to close.
        let class = class_of(fg);
        if class.starts_with("Shell_") || class == "#32768" {
            return;
        }
        // SAFETY: ends this thread's own menu, if one is open.
        unsafe {
            let _ = EndMenu();
        }
    }

    fn language_menu_inner(owner: HWND) -> Option<String> {
        let items = super::languages();
        let Some(anchor) = SHARED.lock().unwrap_or_else(|p| p.into_inner()).anchor else {
            tracing::warn!("language menu: the taskbar button has no position");
            return None;
        };
        if items.is_empty() {
            tracing::warn!("language menu: the language list has not arrived yet");
            return None;
        }
        let current = LANGUAGE.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let current = if current.trim().is_empty() {
            "auto".to_string()
        } else {
            current
        };
        let at = cursor();
        let y = anchor.top - (GAP * anchor.scale as f32).round() as i32;
        // SAFETY: a menu made, used and destroyed on this thread; the owner is
        // this thread's own window.
        unsafe {
            let menu = CreatePopupMenu().ok()?;
            for (i, it) in items.iter().enumerate() {
                let text: Vec<u16> = it.label.encode_utf16().chain(Some(0)).collect();
                let flags = if it.code == current {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING
                };
                let _ = AppendMenuW(menu, flags, i + 1, PCWSTR(text.as_ptr()));
            }
            // A menu closes on a click elsewhere only while the top-level
            // window above its owner is in front. The right click does not
            // always bring the taskbar there, so it is brought there now (this
            // thread shares the taskbar's input and just had the click, so
            // Windows allows it).
            let root = GetAncestor(owner, GA_ROOT);
            if !root.0.is_null() && GetForegroundWindow() != root {
                let _ = SetForegroundWindow(root);
            }
            // And if the front moves anyway (a click on another app, Alt+Tab,
            // a window popping up), the menu closes with it.
            let hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(close_menu_on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            let opened = Instant::now();
            let cmd = TrackPopupMenu(
                menu,
                TPM_BOTTOMALIGN | TPM_CENTERALIGN | TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                at.x,
                y,
                None,
                owner,
                None,
            );
            if !hook.is_invalid() {
                let _ = UnhookWinEvent(hook);
            }
            let n = cmd.0;
            if n <= 0 && opened.elapsed() < Duration::from_millis(250) {
                tracing::warn!(
                    error = %windows::core::Error::from_thread(),
                    "language menu: closed at once without a choice"
                );
            }
            let _ = DestroyMenu(menu);
            // The documented fix for a menu that will not close on the next
            // click elsewhere: wake the owner's queue once it is done.
            let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
            // The app being dictated into gets the focus back.
            restore_foreground();
            (n > 0).then(|| items.get(n as usize - 1).map(|it| it.code.clone()))?
        }
    }

    /// Sets the position, or with `None` forgets it: the button goes back to
    /// the default spot beside the tray, which then becomes its position.
    pub fn set_x(x: Option<i32>) {
        X.store(x.unwrap_or(-1), Ordering::Relaxed);
        let ctl = CTL.load(Ordering::SeqCst);
        if ctl != 0 {
            // SAFETY: as in `stop`.
            unsafe {
                let _ = PostMessageW(Some(hwnd(ctl)), WM_SET_X, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// The thread body: a hidden control window (it gets the broadcasts a child
    /// never sees) and, while a taskbar exists, the button itself.
    fn run() {
        // SAFETY: plain Win32 calls on windows this thread creates and owns.
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else {
                tracing::warn!("taskbar widget: no module handle");
                WANTED.store(false, Ordering::SeqCst);
                return;
            };
            let hinst = HINSTANCE(module.0);
            let ctl_class = w!("OpenLocalDictationTaskbarCtl");
            let widget_class = w!("OpenLocalDictationTaskbarWidget");
            let _ = RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(ctl_proc),
                hInstance: hinst,
                lpszClassName: ctl_class,
                ..Default::default()
            });
            let _ = RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(widget_proc),
                hInstance: hinst,
                lpszClassName: widget_class,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            });
            let Ok(ctl) = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                ctl_class,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinst),
                None,
            ) else {
                tracing::warn!("taskbar widget: control window failed");
                WANTED.store(false, Ordering::SeqCst);
                return;
            };
            L.with(|l| {
                l.created_msg
                    .set(RegisterWindowMessageW(w!("TaskbarCreated")));
                l.light.set(system_uses_light_theme());
                l.shown.set(state_from_code(STATE.load(Ordering::Relaxed)));
                l.started.set(Some(Instant::now()));
                l.last_try.set(None);
                l.warned.set(false);
            });
            CTL.store(raw(ctl), Ordering::SeqCst);
            if !WANTED.load(Ordering::SeqCst) {
                // Stopped before it began: drop the quit message the destroy posts.
                let _ = DestroyWindow(ctl);
                let mut m = MSG::default();
                while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {}
                CTL.store(0, Ordering::SeqCst);
                return;
            }
            create_widget(ctl);
            SetTimer(Some(ctl), T_POLL, POLL_MS, None);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            CTL.store(0, Ordering::SeqCst);
            set_status(false, None);
            let _ = UnregisterClassW(widget_class, Some(hinst));
            let _ = UnregisterClassW(ctl_class, Some(hinst));
        }
    }

    fn system_uses_light_theme() -> bool {
        let mut data = 0u32;
        let mut len = 4u32;
        // SAFETY: the out pointers outlive the call.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("SystemUsesLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut data as *mut u32 as *mut c_void),
                Some(&mut len),
            )
        };
        r.is_ok() && data != 0
    }

    fn find_taskbar() -> Option<HWND> {
        // SAFETY: plain lookup.
        unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok() }
    }

    fn rect_of(h: HWND) -> Option<RECT> {
        let mut r = RECT::default();
        // SAFETY: `r` outlives the call.
        unsafe { GetWindowRect(h, &mut r).ok()? };
        Some(r)
    }

    fn class_of(h: HWND) -> String {
        let mut buf = [0u16; 128];
        // SAFETY: the buffer is passed with its length.
        let n = unsafe { GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// Classes of the taskbar's own parts: the button never avoids those (the
    /// button is meant to sit in the free stretch between them).
    fn is_taskbar_part(class: &str) -> bool {
        matches!(
            class,
            "TrayNotifyWnd"
                | "ReBarWindow32"
                | "MSTaskSwWClass"
                | "MSTaskListWClass"
                | "Start"
                | "TrayDummySearchControl"
                | "OpenLocalDictationTaskbarWidget"
        ) || class.starts_with("Windows.UI.")
    }

    unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> windows::core::BOOL {
        // SAFETY: `lp` is the address of the Vec that `descendants` passed in.
        let list = unsafe { &mut *(lp.0 as *mut Vec<isize>) };
        list.push(raw(h));
        true.into()
    }

    /// Every window below `parent`, at any depth.
    fn descendants(parent: HWND) -> Vec<HWND> {
        let mut list: Vec<isize> = Vec::new();
        // SAFETY: the callback only runs during the call and the Vec outlives it.
        unsafe {
            let _ = EnumChildWindows(
                Some(parent),
                Some(collect),
                LPARAM(&mut list as *mut Vec<isize> as isize),
            );
        }
        list.into_iter().map(hwnd).collect()
    }

    /// Whether `h` sits inside one of the taskbar's own parts (task buttons,
    /// the tray), whose insides are not other widgets.
    fn inside_taskbar_part(h: HWND, taskbar: HWND) -> bool {
        let mut cur = h;
        // SAFETY: plain parent walk, bounded by the depth limit.
        for _ in 0..8 {
            let Ok(parent) = (unsafe { GetParent(cur) }) else {
                return false;
            };
            if parent == taskbar {
                return false;
            }
            if is_taskbar_part(&class_of(parent)) {
                return true;
            }
            cur = parent;
        }
        false
    }

    struct Geom {
        /// Client position inside the taskbar.
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        scale: f32,
        /// Logical px from the taskbar's left edge.
        x_logical: i32,
        /// The position was not set yet: this is the default spot.
        is_default: bool,
        anchor: Anchor,
    }

    /// Works out where the button goes. A saved position is used as it is (so
    /// tray icons coming and going, or other widgets resizing, never move it);
    /// without one, the default spot: just left of the notification area,
    /// stepping left past any other widget parked there. Isolated here because
    /// the taskbar's window layout is undocumented and changes between
    /// Windows builds.
    fn compute_geom(taskbar: HWND, own: isize) -> Result<Geom, &'static str> {
        let bar = rect_of(taskbar).ok_or(NOT_FOUND_REASON)?;
        let (bw, bh) = (bar.right - bar.left, bar.bottom - bar.top);
        if bw <= bh {
            return Err(SIDE_REASON);
        }
        // SAFETY: plain metric query.
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        if bar.top + bh / 2 < screen_h / 2 {
            return Err(SIDE_REASON);
        }
        // SAFETY: plain DPI query.
        let dpi = unsafe { GetDpiForWindow(taskbar) };
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
        let (size, height) = (draw::width_px(scale) as i32, draw::height_px(scale) as i32);
        let gap = (GAP * scale).round() as i32;

        // SAFETY: plain lookup.
        let tray =
            unsafe { FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null()) }
                .map_err(|_| UNRECOGNISED_REASON)?;
        let tray_rect = rect_of(tray).ok_or(UNRECOGNISED_REASON)?;
        if tray_rect.right <= tray_rect.left
            || tray_rect.left <= bar.left
            || tray_rect.left >= bar.right
        {
            return Err(UNRECOGNISED_REASON);
        }

        let fixed = X.load(Ordering::Relaxed);
        let mut others: Vec<RECT> = Vec::new();
        // Every other window in the taskbar's band (other widgets are often
        // not direct children), except the taskbar's own parts and their
        // insides. Only the default spot looks at them.
        for h in descendants(taskbar).into_iter().filter(|_| fixed < 0) {
            if raw(h) == own || !unsafe { IsWindowVisible(h) }.as_bool() {
                continue;
            }
            let Some(r) = rect_of(h) else { continue };
            let width = r.right - r.left;
            if width <= 0 || width >= bw / 2 || r.top >= bar.bottom || r.bottom <= bar.top {
                continue;
            }
            if is_taskbar_part(&class_of(h)) || inside_taskbar_part(h, taskbar) {
                continue;
            }
            others.push(r);
        }
        let mut right = tray_rect.left - gap;
        for _ in 0..16 {
            let Some(r) = others
                .iter()
                .find(|r| r.left < right && r.right > right - size)
            else {
                break;
            };
            right = r.left - gap;
        }

        let left = if fixed >= 0 {
            bar.left + (fixed as f32 * scale).round() as i32
        } else {
            right - size
        }
        .clamp(bar.left, (bar.right - size).max(bar.left));
        let top = bar.top + (bh - height) / 2;
        Ok(Geom {
            x: left - bar.left,
            y: top - bar.top,
            w: size,
            h: height,
            scale,
            x_logical: ((left - bar.left) as f32 / scale).round() as i32,
            is_default: fixed < 0,
            anchor: Anchor {
                center_x: left + size / 2,
                center_y: top + height / 2,
                top: bar.top,
                scale: scale as f64,
            },
        })
    }

    /// Creates the button under the current taskbar. Reports why not on failure.
    fn create_widget(ctl: HWND) {
        let widget = L.with(|l| l.widget.get());
        if widget != 0 {
            return;
        }
        let Some(taskbar) = find_taskbar() else {
            fail(NOT_FOUND_REASON);
            return;
        };
        let geom = match compute_geom(taskbar, 0) {
            Ok(g) => g,
            Err(reason) => {
                fail(reason);
                return;
            }
        };
        // SAFETY: creating a child window of the taskbar on this thread.
        let created = unsafe {
            let Ok(module) = GetModuleHandleW(None) else {
                return;
            };
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE,
                w!("OpenLocalDictationTaskbarWidget"),
                w!("Open Local Dictation"),
                WS_CHILD | WS_CLIPSIBLINGS,
                geom.x,
                geom.y,
                geom.w,
                geom.h,
                Some(taskbar),
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
        };
        let widget = match created {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(error = %e, "taskbar widget: could not create the button");
                fail("The button could not be created in the taskbar.");
                return;
            }
        };
        L.with(|l| {
            l.widget.set(raw(widget));
            l.scale.set(geom.scale);
            l.placed.set(None);
            l.warned.set(false);
        });
        apply_geom(widget, &geom);
        // SAFETY: showing and raising our own window.
        unsafe {
            let _ = ShowWindow(widget, SW_SHOWNA);
            let _ = SetWindowPos(
                widget,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        redraw(widget);
        sync_anim(widget);
        let _ = ctl;
        set_status(true, None);
        tracing::info!("taskbar widget placed");
    }

    fn fail(reason: &str) {
        let first = L.with(|l| !l.warned.replace(true));
        if first {
            tracing::warn!(reason, "taskbar widget unavailable");
        }
        set_status(false, Some(reason));
    }

    fn destroy_widget() {
        let widget = L.with(|l| l.widget.replace(0));
        if widget != 0 {
            // SAFETY: our own window; a window the system already destroyed just fails.
            unsafe {
                let _ = DestroyWindow(hwnd(widget));
            }
        }
        L.with(|l| l.placed.set(None));
    }

    /// Moves the button to `geom` if it is not there yet, and publishes the anchor.
    fn apply_geom(widget: HWND, geom: &Geom) {
        let placed = Placed {
            x: geom.x,
            y: geom.y,
            w: geom.w,
            h: geom.h,
        };
        let moved = L.with(|l| l.placed.replace(Some(placed)) != Some(placed));
        L.with(|l| l.scale.set(geom.scale));
        if geom.is_default {
            // The first placement becomes the position, so it stays put.
            X.store(geom.x_logical, Ordering::Relaxed);
            emit(Event::Moved(geom.x_logical));
        }
        if moved {
            // SAFETY: our own window.
            unsafe {
                let _ = SetWindowPos(
                    widget,
                    None,
                    geom.x,
                    geom.y,
                    geom.w,
                    geom.h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            redraw(widget);
        }
        SHARED.lock().unwrap_or_else(|p| p.into_inner()).anchor = Some(geom.anchor);
    }

    /// Re-places the button, or notices that it lost its taskbar.
    fn reposition(ctl: HWND) {
        let widget = L.with(|l| l.widget.get());
        if widget == 0 {
            let due = L.with(|l| l.last_try.get().is_none_or(|t| t.elapsed() >= RETRY_EVERY));
            if due {
                L.with(|l| l.last_try.set(Some(Instant::now())));
                create_widget(ctl);
            }
            return;
        }
        let w = hwnd(widget);
        // SAFETY: plain queries on our own window.
        let (alive, parent) = unsafe { (IsWindow(Some(w)).as_bool(), GetParent(w).ok()) };
        let taskbar = find_taskbar();
        if !alive || taskbar.is_none() || parent != taskbar {
            destroy_widget();
            set_status(false, Some(NOT_FOUND_REASON));
            return;
        }
        match compute_geom(taskbar.unwrap_or_default(), widget) {
            Ok(g) => {
                apply_geom(w, &g);
                // The taskbar may have raised one of its own parts over the button.
                // SAFETY: plain z-order query and move of our own window.
                unsafe {
                    let top = GetWindow(taskbar.unwrap_or_default(), GW_CHILD).unwrap_or_default();
                    if top != w {
                        let _ = SetWindowPos(
                            w,
                            Some(HWND_TOP),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            Err(reason) => {
                destroy_widget();
                fail(reason);
            }
        }
    }

    /// Starts or stops the animation timer for the state being shown.
    fn sync_anim(widget: HWND) {
        let animated = matches!(
            L.with(|l| l.shown.get()),
            WidgetState::Listening | WidgetState::Busy
        );
        // SAFETY: timers of our own window.
        unsafe {
            if animated {
                SetTimer(Some(widget), T_ANIM, 16, None);
            } else {
                let _ = KillTimer(Some(widget), T_ANIM);
            }
        }
    }

    /// Logical px height of the indicator's letters.
    const LABEL_PX: f32 = 12.0;

    /// Renders `text` white-on-black with GDI (grayscale smoothing) over the
    /// indicator's area and keeps the brightness as coverage: the software
    /// renderer has no fonts of its own.
    fn text_mask(text: &str, scale: f32) -> draw::Mask {
        let h = draw::height_px(scale);
        let w = draw::width_px(scale).saturating_sub(draw::chip_left_px(scale));
        let mut cov = vec![0.0f32; w * h];
        if text.is_empty() || w == 0 {
            return draw::Mask { w, h, cov };
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: GDI objects created here are released before returning; the
        // DIB's memory holds w*h pixels, is zeroed before use and is only read
        // within that.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let mut bits: *mut c_void = std::ptr::null_mut();
            if let Ok(bmp) = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                if !bits.is_null() {
                    std::ptr::write_bytes(bits as *mut u32, 0, w * h);
                    let old = SelectObject(mem, HGDIOBJ(bmp.0));
                    let font = CreateFontW(
                        -((LABEL_PX * scale).round() as i32),
                        0,
                        0,
                        0,
                        600,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        ANTIALIASED_QUALITY,
                        0,
                        w!("Segoe UI"),
                    );
                    let old_font = SelectObject(mem, HGDIOBJ(font.0));
                    SetBkMode(mem, TRANSPARENT);
                    SetTextColor(mem, COLORREF(0x00ff_ffff));
                    let mut wide: Vec<u16> = text.encode_utf16().collect();
                    let mut rect = RECT {
                        left: 0,
                        top: 0,
                        right: w as i32,
                        bottom: h as i32,
                    };
                    DrawTextW(
                        mem,
                        &mut wide,
                        &mut rect,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_NOCLIP,
                    );
                    let _ = GdiFlush();
                    let px = std::slice::from_raw_parts(bits as *const u32, w * h);
                    for (c, p) in cov.iter_mut().zip(px) {
                        *c = ((p >> 8) & 0xff) as f32 / 255.0;
                    }
                    SelectObject(mem, old_font);
                    let _ = DeleteObject(HGDIOBJ(font.0));
                    SelectObject(mem, old);
                }
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
        draw::Mask { w, h, cov }
    }

    /// Draws the current frame into the layered window.
    fn redraw(widget: HWND) {
        let (state, hover, pressed, light, scale, t) = L.with(|l| {
            (
                l.shown.get(),
                l.hover.get(),
                l.pressed.get() && !l.dragging.get(),
                l.light.get(),
                l.scale.get(),
                l.started
                    .get()
                    .map_or(0, |s| s.elapsed().as_millis() as u64),
            )
        });
        let mic_ok = MIC_OK.load(Ordering::Relaxed);
        let text = LABEL.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let (w, h) = (draw::width_px(scale), draw::height_px(scale));
        let chip_w = w.saturating_sub(draw::chip_left_px(scale));
        L.with(|l| {
            let mut cache = l.label.borrow_mut();
            let fresh = matches!(&*cache, Some((s, m)) if *s == text && m.w == chip_w && m.h == h);
            if !fresh {
                *cache = Some((text.clone(), text_mask(&text, scale)));
            }
        });
        let (n, nh, pixels) = L.with(|l| {
            let cache = l.label.borrow();
            draw::render(&draw::Frame {
                state,
                hover,
                pressed,
                light_taskbar: light,
                mic_ok,
                scale,
                t_ms: t,
                label: cache.as_ref().map(|(_, m)| m),
            })
        });
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: n as i32,
                biHeight: -(nh as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: GDI objects created here are released before returning; the
        // DIB's memory holds n*nh pixels and is only written within that.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let mut bits: *mut c_void = std::ptr::null_mut();
            if let Ok(bmp) = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                if !bits.is_null() {
                    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, n * nh);
                    let old = SelectObject(mem, HGDIOBJ(bmp.0));
                    let size = SIZE {
                        cx: n as i32,
                        cy: nh as i32,
                    };
                    let origin = POINT { x: 0, y: 0 };
                    let blend = BLENDFUNCTION {
                        BlendOp: 0,
                        BlendFlags: 0,
                        SourceConstantAlpha: 255,
                        AlphaFormat: 1,
                    };
                    let _ = UpdateLayeredWindow(
                        widget,
                        Some(screen),
                        None,
                        Some(&size),
                        Some(mem),
                        Some(&origin),
                        COLORREF(0),
                        Some(&blend),
                        ULW_ALPHA,
                    );
                    SelectObject(mem, old);
                }
                let _ = DeleteObject(HGDIOBJ(bmp.0));
            }
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
    }

    /// The browser's own grab and grabbing cursors (Chromium's `hand_grab.cur`
    /// and `hand_grabbing.cur`, BSD licensed): the ones the pill's CSS
    /// `cursor: grab` / `grabbing` show, so the button feels the same.
    const GRAB_CUR: &[u8] = include_bytes!("../assets/hand_grab.cur");
    const GRABBING_CUR: &[u8] = include_bytes!("../assets/hand_grabbing.cur");

    thread_local! {
        /// Open and closed hand, built once on first use (raw handles; 0 = failed).
        static HANDS: Cell<Option<(isize, isize)>> = const { Cell::new(None) };
    }

    /// Reads the largest image of a `.cur` file and its hotspot, and makes a
    /// cursor of it. `CreateIconFromResourceEx` wants the hotspot (two u16)
    /// in front of the image data, as in a cursor resource.
    fn cursor_from_cur(file: &[u8]) -> isize {
        let u16_at = |o: usize| file.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| {
            file.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let count = u16_at(4).unwrap_or(0) as usize;
        let best = (0..count)
            .filter_map(|i| {
                let e = 6 + 16 * i;
                Some((
                    u32_at(e + 8)?,
                    u16_at(e + 4)?,
                    u16_at(e + 6)?,
                    u32_at(e + 12)?,
                ))
            })
            .max_by_key(|&(size, ..)| size);
        let Some((size, hx, hy, off)) = best else {
            return 0;
        };
        let Some(image) = file.get(off as usize..(off + size) as usize) else {
            return 0;
        };
        let mut data = Vec::with_capacity(image.len() + 4);
        data.extend_from_slice(&hx.to_le_bytes());
        data.extend_from_slice(&hy.to_le_bytes());
        data.extend_from_slice(image);
        // SAFETY: `data` is a complete cursor resource and outlives the call
        // (the cursor copies it).
        unsafe {
            CreateIconFromResourceEx(&data, false, 0x0003_0000, 0, 0, LR_DEFAULTCOLOR)
                .map(|i| i.0 as isize)
                .unwrap_or(0)
        }
    }

    /// The grab cursor (`closed`: the button is held), or the move cursor when
    /// the hand could not be built.
    fn grab_cursor(closed: bool) -> Option<HCURSOR> {
        let (open, fist) = HANDS.with(|h| {
            if h.get().is_none() {
                h.set(Some((
                    cursor_from_cur(GRAB_CUR),
                    cursor_from_cur(GRABBING_CUR),
                )));
            }
            h.get().unwrap_or((0, 0))
        });
        let raw = if closed { fist } else { open };
        if raw != 0 {
            return Some(HCURSOR(raw as *mut c_void));
        }
        // SAFETY: loading a stock cursor.
        unsafe { LoadCursorW(None, IDC_SIZEALL).ok() }
    }

    fn show_state(next: WidgetState) {
        let widget = L.with(|l| {
            l.shown.set(next);
            l.widget.get()
        });
        if widget != 0 {
            sync_anim(hwnd(widget));
            redraw(hwnd(widget));
        }
    }

    unsafe extern "system" fn ctl_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // SAFETY: standard window procedure of a window this thread owns.
        unsafe {
            if msg != 0 && msg == L.with(|l| l.created_msg.get()) {
                // Explorer restarted: the old parent is gone. Build the button
                // again once the new taskbar has its parts.
                destroy_widget();
                set_status(false, Some(NOT_FOUND_REASON));
                L.with(|l| l.last_try.set(None));
                SetTimer(Some(h), T_POLL, 1000, None);
                return LRESULT(0);
            }
            match msg {
                WM_SET_STATE => {
                    let next = state_from_code(wp.0 as u8);
                    let lingering = L
                        .with(|l| matches!(l.shown.get(), WidgetState::Done | WidgetState::Fail))
                        && next == WidgetState::Idle;
                    if next == WidgetState::Idle {
                        if !lingering {
                            SetTimer(Some(h), T_IDLE, IDLE_DELAY_MS, None);
                        }
                    } else {
                        let _ = KillTimer(Some(h), T_IDLE);
                        let _ = KillTimer(Some(h), T_LINGER);
                        show_state(next);
                        match next {
                            WidgetState::Done => {
                                SetTimer(Some(h), T_LINGER, crate::LINGER_OK_MS as u32, None);
                            }
                            WidgetState::Fail => {
                                SetTimer(Some(h), T_LINGER, crate::LINGER_MSG_MS as u32, None);
                            }
                            _ => {}
                        }
                    }
                    LRESULT(0)
                }
                WM_SET_X => {
                    reposition(h);
                    LRESULT(0)
                }
                WM_TIMER => {
                    match wp.0 {
                        T_POLL => {
                            // Settles at the normal pace after a restart's quick first tick.
                            SetTimer(Some(h), T_POLL, POLL_MS, None);
                            reposition(h);
                        }
                        T_IDLE | T_LINGER => {
                            let _ = KillTimer(Some(h), wp.0);
                            show_state(WidgetState::Idle);
                        }
                        _ => {}
                    }
                    LRESULT(0)
                }
                WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
                    L.with(|l| l.light.set(system_uses_light_theme()));
                    let widget = L.with(|l| l.widget.get());
                    if widget != 0 {
                        redraw(hwnd(widget));
                    }
                    reposition(h);
                    LRESULT(0)
                }
                WM_MIC | WM_LANG => {
                    let widget = L.with(|l| l.widget.get());
                    if widget != 0 {
                        redraw(hwnd(widget));
                    }
                    LRESULT(0)
                }
                WM_STOP => {
                    let _ = DestroyWindow(h);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    destroy_widget();
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(h, msg, wp, lp),
            }
        }
    }

    fn cursor() -> POINT {
        let mut p = POINT::default();
        // SAFETY: `p` outlives the call.
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        p
    }

    unsafe extern "system" fn widget_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // SAFETY: standard window procedure of a window this thread owns.
        unsafe {
            match msg {
                // Clicking must never activate the taskbar: the app being
                // dictated into has to keep the keyboard focus.
                // The click makes the taskbar the foreground window (refusing that
                // leaves no foreground window at all), and the old one is already
                // gone by the time this arrives. So the focus is noted on the way
                // in, and handed back once the click is done.
                WM_MOUSEACTIVATE => DefWindowProcW(h, msg, wp, lp),
                WM_MOUSEMOVE => {
                    if !L.with(|l| l.pressed.get()) {
                        note_foreground();
                    }
                    if !L.with(|l| l.tracking.replace(true)) {
                        let mut ev = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: h,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut ev);
                    }
                    // Only the mic disc opens the pill; resting on the language
                    // indicator does not (and leaving the disc for it closes it).
                    let mx = (lp.0 & 0xffff) as i16 as i32;
                    let on_chip = mx >= L.with(|l| l.placed.get().map_or(i32::MAX, |p| p.h));
                    let entered = !L.with(|l| l.hover.replace(true));
                    let changed = L.with(|l| l.on_chip.replace(on_chip)) != on_chip;
                    if entered || changed {
                        if on_chip {
                            let _ = KillTimer(Some(h), T_HOVER);
                            if L.with(|l| l.hover_fired.replace(false)) {
                                emit(Event::HoverEnd);
                            }
                        } else if !L.with(|l| l.pressed.get()) {
                            SetTimer(Some(h), T_HOVER, HOVER_MS, None);
                        }
                        redraw(h);
                    }
                    if L.with(|l| l.pressed.get() && l.drag_armed.get()) {
                        let (start_x, start_px, scale) =
                            L.with(|l| (l.drag_x.get(), l.drag_offset.get(), l.scale.get()));
                        let dx = cursor().x - start_x;
                        if dx.abs() > DRAG_SLOP || L.with(|l| l.dragging.get()) {
                            L.with(|l| l.dragging.set(true));
                            let x = ((start_px + dx) as f32 / scale).round() as i32;
                            X.store(x.max(0), Ordering::Relaxed);
                            reposition(hwnd(CTL.load(Ordering::SeqCst)));
                        }
                    }
                    LRESULT(0)
                }
                WM_MOUSELEAVE => {
                    let fired = L.with(|l| {
                        l.tracking.set(false);
                        l.hover.set(false);
                        l.on_chip.set(false);
                        l.hover_fired.replace(false)
                    });
                    let _ = KillTimer(Some(h), T_HOVER);
                    if fired {
                        emit(Event::HoverEnd);
                    }
                    redraw(h);
                    LRESULT(0)
                }
                WM_TIMER => {
                    match wp.0 {
                        T_HOVER => {
                            let _ = KillTimer(Some(h), T_HOVER);
                            if L.with(|l| l.hover.get() && !l.pressed.get() && !l.on_chip.get()) {
                                L.with(|l| l.hover_fired.set(true));
                                emit(Event::HoverStart);
                            }
                        }
                        T_ANIM => redraw(h),
                        _ => {}
                    }
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    // The button only moves when Ctrl is held at the press; a plain
                    // press is always a click, however far the pointer travels.
                    let ctrl = (wp.0 & MK_CONTROL) != 0;
                    L.with(|l| {
                        l.pressed.set(true);
                        l.drag_armed.set(ctrl);
                        l.dragging.set(false);
                        l.drag_x.set(cursor().x);
                        l.drag_offset.set(l.placed.get().map_or(0, |p| p.x));
                    });
                    let _ = KillTimer(Some(h), T_HOVER);
                    SetCapture(h);
                    redraw(h);
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    let (was, moved) = L.with(|l| (l.pressed.replace(false), l.dragging.get()));
                    let _ = ReleaseCapture();
                    redraw(h);
                    if was {
                        restore_foreground();
                        if moved {
                            // A drag ended: save where the button now is (after the
                            // clamp into the taskbar).
                            let (x, scale) =
                                L.with(|l| (l.placed.get().map_or(0, |p| p.x), l.scale.get()));
                            let x = (x as f32 / scale).round() as i32;
                            X.store(x, Ordering::Relaxed);
                            emit(Event::Moved(x));
                        } else {
                            // Released over the button (capture reports the point even outside).
                            let (x, y) = (
                                (lp.0 & 0xffff) as i16 as i32,
                                ((lp.0 >> 16) & 0xffff) as i16 as i32,
                            );
                            let (w, h) = L.with(|l| l.placed.get().map_or((0, 0), |p| (p.w, p.h)));
                            if x >= 0 && y >= 0 && x < w && y < h {
                                emit(Event::Clicked);
                            }
                        }
                    }
                    LRESULT(0)
                }
                // Right click: the language menu. Like a left click it makes the
                // taskbar the foreground window, so the focus is handed back.
                WM_RBUTTONUP => {
                    // The focus is handed back when the menu closes, not now:
                    // moving it while the menu opens would close the menu.
                    // No hover pill while the menu is up.
                    let _ = KillTimer(Some(h), T_HOVER);
                    if let Some(code) = language_menu(h) {
                        emit(Event::LanguagePicked(code));
                    }
                    LRESULT(0)
                }
                WM_SETCURSOR => {
                    // An open hand while Ctrl is held over the button (it can be grabbed),
                    // a closed one from the press until the button is let go.
                    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                    let held = L.with(|l| l.pressed.get() && l.drag_armed.get());
                    if held || ctrl {
                        if let Some(c) = grab_cursor(held) {
                            SetCursor(Some(c));
                            return LRESULT(1);
                        }
                    }
                    DefWindowProcW(h, msg, wp, lp)
                }
                WM_NCDESTROY => {
                    // Gone with the taskbar (Explorer exited) or destroyed by us.
                    L.with(|l| {
                        if l.widget.get() == raw(h) {
                            l.widget.set(0);
                            l.placed.set(None);
                        }
                    });
                    DefWindowProcW(h, msg, wp, lp)
                }
                _ => DefWindowProcW(h, msg, wp, lp),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_grab_cursors_load() {
            assert_ne!(cursor_from_cur(GRAB_CUR), 0);
            assert_ne!(cursor_from_cur(GRABBING_CUR), 0);
            assert_eq!(cursor_from_cur(&[0, 0, 2, 0, 5, 0]), 0);
        }
    }
}
