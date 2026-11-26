//! Settings stored as one JSON file in the app data folder. A file (not a
//! database) because the GPU switch has to be read before Tauri starts.

use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// "" = the best installed model, otherwise a catalog id or a custom model id.
    pub model: String,
    /// Folder the models are read from and downloaded to; "" = the app's own
    /// folder. Set only through `models_set_dir`, so it can be shared with other apps.
    pub models_dir: String,
    /// "gpu" | "cpu". GPU only takes effect once the CUDA pack is installed.
    pub device: String,
    /// "auto" or a language code.
    pub language: String,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: String::new(),
            models_dir: String::new(),
            device: "gpu".into(),
            language: "auto".into(),
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
    /// Clamps values into sane ranges; applied on every load and save.
    pub fn sanitize(&mut self) {
        self.device = pick(&self.device, &["gpu", "cpu"], "gpu");
        self.mode = pick(&self.mode, &["hold", "toggle"], "hold");
        self.insert_method = pick(&self.insert_method, &["type", "paste"], "type");
        self.theme = pick(&self.theme, &["system", "light", "dark"], "system");
        if self.language.trim().is_empty() {
            self.language = "auto".into();
        }
        if self.shortcut.trim().is_empty() {
            self.shortcut = Settings::default().shortcut;
        }
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
        s.sanitize();
        assert_eq!(s.device, "gpu");
        assert_eq!(s.silence_ms, 300);
        assert_eq!(s.language, "auto");
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
