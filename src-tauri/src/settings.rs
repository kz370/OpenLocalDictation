//! Settings stored as one JSON file in the app data folder. A file (not a
//! database) because the GPU switch has to be read before Tauri starts.

use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE: &str = "settings.json";

/// Limits of the small overlay's width (logical px).
pub const MINI_MIN_WIDTH: u32 = 320;
pub const MINI_MAX_WIDTH: u32 = 800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "" = the best installed model, otherwise a catalog id or a custom model id.
    pub model: String,
    /// Model per language code ("" or missing = use `model`). "auto" has no
    /// entry of its own: the language is unknown until the take is decoded.
    pub language_models: BTreeMap<String, String>,
    /// Folder the models are read from and downloaded to; "" = the app's own
    /// folder. Set only through `models_set_dir`, so it can be shared with other apps.
    pub models_dir: String,
    /// "gpu" | "cpu". GPU only takes effect once the CUDA pack is installed.
    pub device: String,
    /// "auto" or a language code.
    pub language: String,
    /// Language codes listed in the overlay's language menu; empty = all of them.
    pub shown_languages: Vec<String>,
    /// None = system default microphone.
    pub microphone: Option<String>,
    pub shortcut: String,
    /// "hold" | "toggle"
    pub mode: String,
    /// "type" | "paste"
    pub insert_method: String,
    pub add_trailing_space: bool,
    /// Hold the result in the overlay for editing before it is inserted.
    pub review_before_insert: bool,
    pub start_with_os: bool,
    /// "system" | "light" | "dark"
    pub theme: String,
    pub vad_threshold: f32,
    pub silence_ms: u32,
    /// Stop recording after this much silence (seconds, 0 = never).
    pub auto_stop_silence_secs: u32,
    pub overlay_x: Option<i32>,
    pub overlay_y: Option<i32>,
    /// Keep the overlay on screen (always on top) between dictations. It never
    /// listens while idle: only the shortcut or its mic button start recording.
    pub overlay_pinned: bool,
    /// Use the small overlay (mic and level only; review still opens full size).
    pub overlay_mini: bool,
    /// Which overlay sizes may be used: "small" | "large" | "both". With "both",
    /// `overlay_mini` is the one currently shown (toggled from the overlay).
    pub overlay_layouts: String,
    /// Overlay background opacity in percent (30-100).
    pub overlay_opacity: u32,
    /// Show the settings button in the overlay header.
    pub overlay_show_settings: bool,
    /// The overlay can only be dragged sideways; it stays just above the taskbar.
    pub overlay_horizontal_only: bool,
    /// The overlay cannot be dragged at all.
    pub overlay_position_locked: bool,
    /// Width of the small overlay in logical px (dragged on its right edge).
    pub overlay_mini_width: u32,
    /// Bubble style: how far (logical px, + = right) the mic circle sits from the
    /// window's center. The pill stays inside the screen; the circle may reach the edge.
    pub overlay_orb_shift: i32,
    /// Locks the small overlay's width: no resize handle.
    pub overlay_size_locked: bool,
    /// Look of the small overlay: "pill" (one pill with the mic inside, always
    /// shown) | "bubble" (a mic circle; the pill shows above it on hover and
    /// while dictating).
    pub overlay_style: String,
    /// Show a mic button inside the Windows taskbar (bottom taskbar only). The
    /// pill then opens above it instead of above a floating circle.
    pub taskbar_widget: bool,
    /// Where the taskbar button sits: logical px from the taskbar's left edge.
    /// `None` until it was first placed; then fixed until the user drags it.
    pub taskbar_widget_x: Option<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: String::new(),
            language_models: BTreeMap::new(),
            models_dir: String::new(),
            device: "gpu".into(),
            language: "auto".into(),
            shown_languages: Vec::new(),
            microphone: None,
            shortcut: "CommandOrControl+Alt+Space".into(),
            mode: "hold".into(),
            insert_method: "type".into(),
            add_trailing_space: true,
            review_before_insert: true,
            start_with_os: false,
            theme: "system".into(),
            vad_threshold: 0.6,
            silence_ms: 800,
            auto_stop_silence_secs: 8,
            overlay_x: None,
            overlay_y: None,
            overlay_pinned: false,
            overlay_mini: false,
            overlay_layouts: "small".into(),
            overlay_opacity: 100,
            overlay_show_settings: true,
            overlay_horizontal_only: true,
            overlay_position_locked: false,
            overlay_mini_width: 400,
            overlay_orb_shift: 0,
            overlay_size_locked: false,
            overlay_style: "pill".into(),
            taskbar_widget: false,
            taskbar_widget_x: None,
        }
    }
}

fn pick<'a>(v: &'a str, allowed: &[&'a str], fallback: &'a str) -> String {
    if allowed.contains(&v) {
        v.to_string()
    } else {
        fallback.to_string()
    }
}

impl Settings {
    /// Whether the overlay uses its small layout right now.
    pub fn effective_mini(&self) -> bool {
        match self.overlay_layouts.as_str() {
            "small" => true,
            "large" => false,
            _ => self.overlay_mini,
        }
    }

    /// The model id to use for the current language: its own choice when the
    /// user made one, otherwise the model for all languages (`model`).
    pub fn effective_model(&self) -> &str {
        self.language_models
            .get(self.language.trim())
            .map(|m| m.as_str())
            .filter(|m| !m.trim().is_empty())
            .unwrap_or(&self.model)
    }

    /// Clamps values into sane ranges; applied on every load and save.
    pub fn sanitize(&mut self) {
        self.device = pick(&self.device, &["gpu", "cpu"], "gpu");
        self.mode = pick(&self.mode, &["hold", "toggle"], "hold");
        self.insert_method = pick(&self.insert_method, &["type", "paste"], "type");
        self.theme = pick(&self.theme, &["system", "light", "dark"], "system");
        if self.language.trim().is_empty() {
            self.language = "auto".into();
        }
        self.language_models.retain(|lang, model| {
            !lang.trim().is_empty() && lang != "auto" && !model.trim().is_empty()
        });
        let mut seen = std::collections::HashSet::new();
        self.shown_languages
            .retain(|c| !c.trim().is_empty() && seen.insert(c.clone()));
        if self.shortcut.trim().is_empty() {
            self.shortcut = Settings::default().shortcut;
        }
        self.overlay_layouts = pick(&self.overlay_layouts, &["small", "large", "both"], "small");
        self.overlay_style = pick(&self.overlay_style, &["pill", "bubble"], "pill");
        self.overlay_opacity = self.overlay_opacity.clamp(30, 100);
        self.overlay_mini_width = self
            .overlay_mini_width
            .clamp(MINI_MIN_WIDTH, MINI_MAX_WIDTH);
        self.overlay_orb_shift = self.overlay_orb_shift.clamp(-2000, 2000);
        self.taskbar_widget_x = self.taskbar_widget_x.map(|x| x.clamp(0, 16_000));
        self.vad_threshold = self.vad_threshold.clamp(0.2, 0.95);
        self.silence_ms = self.silence_ms.clamp(300, 3000);
        self.auto_stop_silence_secs = self.auto_stop_silence_secs.min(120);
        if self.microphone.as_deref() == Some("") {
            self.microphone = None;
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE);
        let mut s = read(data_dir);
        s.sanitize();
        Self {
            path,
            current: Mutex::new(s),
        }
    }

    pub fn get(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> AppResult<Settings> {
        let mut guard = self.current.lock().unwrap_or_else(|p| p.into_inner());
        let mut next = guard.clone();
        f(&mut next);
        next.sanitize();
        write_file(&self.path, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

/// Reads the settings file; a missing or damaged file yields the defaults.
pub fn read(data_dir: &Path) -> Settings {
    std::fs::read_to_string(data_dir.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_file(path: &Path, s: &Settings) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(&tmp, path).map_err(AppError::from)
}

/// Writes only the device choice. Used before Tauri starts (crash guard) and
/// by the GPU module, which cannot reach the running store.
pub fn write_device(data_dir: &Path, device: &str) -> AppResult<()> {
    let mut s = read(data_dir);
    s.device = device.into();
    write_file(&data_dir.join(FILE), &s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_review_on_and_sanitize_clamps() {
        let mut s = Settings::default();
        assert!(s.review_before_insert);
        s.device = "tpu".into();
        s.silence_ms = 5;
        s.language = " ".into();
        s.shown_languages = vec!["ar".into(), "".into(), "ar".into(), "de".into()];
        s.sanitize();
        assert_eq!(s.shown_languages, vec!["ar", "de"]);
        assert_eq!(s.device, "gpu");
        assert_eq!(s.silence_ms, 300);
        assert_eq!(s.language, "auto");
    }

    #[test]
    fn model_follows_the_language_when_one_is_set() {
        let mut s = Settings {
            model: "whisper-base".into(),
            language: "ar".into(),
            ..Settings::default()
        };
        assert_eq!(s.effective_model(), "whisper-base");
        s.language_models
            .insert("ar".into(), "whisper-turbo".into());
        s.language_models
            .insert("auto".into(), "whisper-tiny".into());
        s.language_models.insert("de".into(), " ".into());
        s.sanitize();
        assert_eq!(s.effective_model(), "whisper-turbo");
        assert_eq!(s.language_models.len(), 1);
        s.language = "en".into();
        assert_eq!(s.effective_model(), "whisper-base");
        s.language = "auto".into();
        assert_eq!(s.effective_model(), "whisper-base");
    }

    #[test]
    fn taskbar_widget_is_off_by_default_and_position_is_clamped() {
        let mut s = Settings::default();
        assert!(!s.taskbar_widget);
        assert_eq!(s.taskbar_widget_x, None);
        s.taskbar_widget_x = Some(-50);
        s.sanitize();
        assert_eq!(s.taskbar_widget_x, Some(0));
        s.taskbar_widget_x = Some(99_999);
        s.sanitize();
        assert_eq!(s.taskbar_widget_x, Some(16_000));
    }

    #[test]
    fn round_trips_and_survives_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path());
        store.update(|s| s.language = "ar".into()).unwrap();
        assert_eq!(SettingsStore::load(dir.path()).get().language, "ar");
        write_device(dir.path(), "cpu").unwrap();
        assert_eq!(read(dir.path()).device, "cpu");
        assert_eq!(read(dir.path()).language, "ar");
        std::fs::write(dir.path().join(FILE), b"{ nope").unwrap();
        assert_eq!(read(dir.path()), Settings::default());
    }
}
