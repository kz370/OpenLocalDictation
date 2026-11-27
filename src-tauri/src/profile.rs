//! Color profiles: the accent color of the tray icon, the taskbar button and
//! the pages. This table is the one source: the pages get it through the
//! `color_profiles` command (they build their CSS variables and recolor the
//! app icon from it). Red, the "no usable microphone" color, belongs to no
//! profile and is never changed by one.

use serde::Serialize;

/// One color profile.
pub struct Profile {
    pub id: &'static str,
    pub label: &'static str,
    /// Accent on a light / dark background (0xRRGGBB) and the color of what is
    /// drawn on it. The taskbar button picks by the taskbar's theme.
    pub accent_light: u32,
    pub ink_light: u32,
    pub accent_dark: u32,
    pub ink_dark: u32,
    /// How the app icon (green, with white bars) is recolored: `None` keeps it;
    /// otherwise its hue turns to `hue` degrees, its saturation is multiplied by
    /// `sat` and its lightness by `light`. White stays white.
    pub tray: Option<TrayTint>,
}

#[derive(Clone, Copy, Serialize)]
pub struct TrayTint {
    pub hue: f32,
    pub sat: f32,
    pub light: f32,
}

pub const DEFAULT: &str = "teal";

const fn tint(hue: f32, sat: f32, light: f32) -> Option<TrayTint> {
    Some(TrayTint { hue, sat, light })
}

#[allow(clippy::too_many_arguments)]
const fn profile(
    id: &'static str,
    label: &'static str,
    accent_light: u32,
    ink_light: u32,
    accent_dark: u32,
    ink_dark: u32,
    tray: Option<TrayTint>,
) -> Profile {
    Profile {
        id,
        label,
        accent_light,
        ink_light,
        accent_dark,
        ink_dark,
        tray,
    }
}

/// The default first. Five accents that are clearly different from one another;
/// grey is `slate`. No red or pink: red is the microphone alert (tray icon and
/// taskbar button) and the listening disc, and no accent may be mistaken for it.
pub const PROFILES: [Profile; 5] = [
    profile(
        "teal",
        "Teal",
        0x0f766e,
        0xffffff,
        0x2dd4bf,
        0x04211e,
        tint(175.0, 0.85, 1.0),
    ),
    profile(
        "blue",
        "Blue",
        0x2563eb,
        0xffffff,
        0x60a5fa,
        0x0b1b3a,
        tint(217.0, 0.95, 1.0),
    ),
    // The app icon is green already: it stays as it is.
    profile(
        "green", "Green", 0x15803d, 0xffffff, 0x4ade80, 0x052e14, None,
    ),
    profile(
        "amber",
        "Amber",
        0xb45309,
        0xffffff,
        0xfbbf24,
        0x2b1b02,
        tint(38.0, 1.0, 1.1),
    ),
    profile(
        "slate",
        "Slate",
        0x475569,
        0xffffff,
        0x94a3b8,
        0x0f172a,
        tint(215.0, 0.25, 1.0),
    ),
];

/// A profile as the pages see it (`color_profiles`): colors as `#rrggbb`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub accent_light: String,
    pub ink_light: String,
    pub accent_dark: String,
    pub ink_dark: String,
    pub tint: Option<TrayTint>,
}

fn css(c: u32) -> String {
    format!("#{c:06x}")
}

pub fn infos() -> Vec<ProfileInfo> {
    PROFILES
        .iter()
        .map(|p| ProfileInfo {
            id: p.id,
            label: p.label,
            accent_light: css(p.accent_light),
            ink_light: css(p.ink_light),
            accent_dark: css(p.accent_dark),
            ink_dark: css(p.ink_dark),
            tint: p.tray,
        })
        .collect()
}

/// Position of `id` in `PROFILES`; the default's for an unknown one.
pub fn index(id: &str) -> usize {
    PROFILES.iter().position(|p| p.id == id).unwrap_or(0)
}

/// The ids a setting may hold.
pub fn ids() -> Vec<&'static str> {
    PROFILES.iter().map(|p| p.id).collect()
}

/// Recolors an RGBA icon (4 bytes per pixel) to `tint`. Pure, so it can be tested.
pub fn tint_icon(rgba: &[u8], tint: TrayTint) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        let (r, g, b) = (
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        );
        let (_, s, l) = rgb_to_hsl(r, g, b);
        // The white bars (and anything nearly grey) are kept as they are.
        if s < 0.12 {
            continue;
        }
        let (nr, ng, nb) = hsl_to_rgb(
            tint.hue,
            (s * tint.sat).clamp(0.0, 1.0),
            (l * tint.light).clamp(0.0, 1.0),
        );
        px[0] = (nr * 255.0).round() as u8;
        px[1] = (ng * 255.0).round() as u8;
        px[2] = (nb * 255.0).round() as u8;
    }
    out
}

/// Hue in degrees, saturation and lightness in 0..1.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_first_and_unknown_ids_fall_back_to_it() {
        assert_eq!(PROFILES[0].id, DEFAULT);
        assert_eq!(index("blue"), 1);
        assert_eq!(index("purple"), 0);
        assert_eq!(index(""), 0);
    }

    #[test]
    fn the_icon_turns_blue_and_its_white_bars_stay_white() {
        let green = [63, 224, 119, 255, 255, 255, 255, 255, 0, 0, 0, 0];
        let out = tint_icon(&green, PROFILES[index("blue")].tray.unwrap());
        // Blue now dominates the colored pixel.
        assert!(out[2] > out[0] && out[2] > out[1], "{:?}", &out[..4]);
        assert_eq!(out[3], 255);
        assert_eq!(&out[4..8], &[255, 255, 255, 255]);
        assert_eq!(out[11], 0);
    }

    #[test]
    fn slate_is_nearly_grey_and_amber_is_warm() {
        let green = [63, 224, 119, 255];
        let g = tint_icon(&green, PROFILES[index("slate")].tray.unwrap());
        assert!(g[0].abs_diff(g[2]) < 80, "{g:?}");
        let a = tint_icon(&green, PROFILES[index("amber")].tray.unwrap());
        assert!(a[0] > a[2] && a[1] > a[2], "{a:?}");
    }

    /// WCAG contrast ratio of two 0xRRGGBB colors.
    fn contrast(a: u32, b: u32) -> f32 {
        let lum = |c: u32| {
            let f = |v: u32| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c >> 16 & 0xff) + 0.7152 * f(c >> 8 & 0xff) + 0.0722 * f(c & 0xff)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn what_is_drawn_on_an_accent_is_readable() {
        for p in &PROFILES {
            assert!(
                contrast(p.accent_light, p.ink_light) >= 4.5,
                "{} light",
                p.id
            );
            assert!(contrast(p.accent_dark, p.ink_dark) >= 4.5, "{} dark", p.id);
        }
    }

    #[test]
    fn no_accent_is_close_to_the_alert_red() {
        for p in &PROFILES {
            for c in [p.accent_light, p.accent_dark] {
                let (r, g, b) = (
                    (c >> 16 & 0xff) as f32,
                    (c >> 8 & 0xff) as f32,
                    (c & 0xff) as f32,
                );
                let (h, s, _) = rgb_to_hsl(r / 255.0, g / 255.0, b / 255.0);
                // Hues from pink (320) through red to a reddish orange (20).
                let reddish = h >= 320.0 || h <= 20.0;
                assert!(!reddish || s < 0.2, "{} is too close to red", p.id);
            }
            if let Some(t) = p.tray {
                let reddish = t.hue >= 320.0 || t.hue <= 20.0;
                assert!(
                    !reddish || t.sat < 0.2,
                    "{} tray icon is too close to red",
                    p.id
                );
            }
        }
    }

    #[test]
    fn ids_and_colors_are_unique() {
        let mut ids = std::collections::HashSet::new();
        let mut colors = std::collections::HashSet::new();
        for p in &PROFILES {
            assert!(ids.insert(p.id), "{} twice", p.id);
            assert!(colors.insert(p.accent_dark), "{} repeats a color", p.id);
        }
    }
}
