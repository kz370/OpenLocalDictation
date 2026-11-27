//! Local speech-to-text (Whisper, or a model the user added). Audio never leaves the machine.

pub mod engine;
pub mod session;

use crate::errors::{AppError, AppResult};
use crate::hardware::HardwareInfo;
use crate::models::ModelStore;
use crate::settings::Settings;
use engine::{EngineOptions, ModelFiles, Recognizer};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A speech model that is in memory, and where it runs.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModel {
    pub id: String,
    pub name: String,
    /// "Whisper", "Moonshine", ...
    pub kind: &'static str,
    /// "gpu" | "cpu"
    pub device: &'static str,
    /// The GPU is on but this model's format cannot use it, so it runs on the CPU.
    pub gpu_blocked: bool,
    /// Language code a Whisper model decodes, "" = it detects (or the family has none).
    pub language: String,
}

struct Loaded {
    files: ModelFiles,
    recognizer: Recognizer,
    /// Language the recognizer currently decodes.
    language: String,
}

/// One model in memory. `info` can be read while `inner` is busy decoding.
struct Entry {
    key: String,
    info: Mutex<LoadedModel>,
    inner: Mutex<Loaded>,
}

pub struct SttService {
    store: Arc<ModelStore>,
    hw: HardwareInfo,
    /// Every model in memory; loaded on first use or by `preload`, kept until
    /// `unload_model` / `unload` (or a device change).
    entries: Mutex<Vec<Arc<Entry>>>,
    /// Serializes loading, so a preload and a take never load the same model twice.
    load_lock: Mutex<()>,
}

/// Minimum audio length worth transcribing (models hallucinate on near-silence).
pub const MIN_AUDIO_MS: u64 = 350;

/// "auto" lets Whisper detect the language; anything else is a language code.
pub fn whisper_language(setting: &str) -> String {
    match setting.trim() {
        "auto" | "" => String::new(),
        code => code.to_ascii_lowercase(),
    }
}

impl SttService {
    pub fn new(store: Arc<ModelStore>, hw: HardwareInfo) -> Self {
        Self {
            store,
            hw,
            entries: Mutex::new(Vec::new()),
            load_lock: Mutex::new(()),
        }
    }

    pub fn is_ready(&self, settings: &Settings) -> bool {
        self.store.resolve(settings.effective_model()).is_some()
    }

    /// The models in memory (nothing is loaded until start-up preloading or the
    /// first take has done it).
    pub fn loaded_models(&self) -> Vec<LoadedModel> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|e| e.info.lock().unwrap_or_else(|p| p.into_inner()).clone())
            .collect()
    }

    /// Drops every recognizer (the device changed).
    pub fn unload(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    /// Frees one model's memory. A take that is decoding with it right now
    /// finishes first (it holds its own reference). The files are never touched.
    pub fn unload_model(&self, id: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|e| e.info.lock().unwrap_or_else(|p| p.into_inner()).id != id);
    }

    /// Loads the models chosen for start-up (`auto_load_models`; never chosen:
    /// the default model and the one for the dictation language), so the first
    /// take does not wait for them. Blocks for the load; run it on a thread.
    pub fn preload(&self, settings: &Settings) {
        let ids: Vec<String> = match &settings.auto_load_models {
            // Only models that are really there: a missing one must not make
            // `resolve` stand in with another.
            Some(list) => list
                .iter()
                .filter(|id| self.store.resolve(id).is_some_and(|m| &m.id == *id))
                .cloned()
                .collect(),
            None => {
                let mut v: Vec<String> = Vec::new();
                for chosen in [settings.effective_model(), settings.model.as_str()] {
                    if let Some(m) = self.store.resolve(chosen) {
                        if !v.contains(&m.id) {
                            v.push(m.id);
                        }
                    }
                }
                v
            }
        };
        for id in ids {
            if let Err(e) = self.with_model(settings, &id, |_| ()) {
                tracing::warn!(error = %e, "could not preload a speech model");
            }
        }
    }

    /// Loads one model now (the Load button), on the current device. Blocks for
    /// the load; run it on a thread.
    pub fn load_model(&self, settings: &Settings, id: &str) -> AppResult<()> {
        self.with_model(settings, id, |_| ())
    }

    /// Runs `f` with the recognizer of the model for the current language,
    /// loading it first if needed. Models stay loaded for the next call; a
    /// Whisper language change switches in place instead of reloading.
    pub fn with_recognizer<R>(
        &self,
        settings: &Settings,
        f: impl FnOnce(&Recognizer) -> R,
    ) -> AppResult<R> {
        self.with_model(settings, settings.effective_model(), f)
    }

    fn find(&self, key: &str) -> Option<Arc<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|e| e.key == key)
            .cloned()
    }

    fn with_model<R>(
        &self,
        settings: &Settings,
        chosen: &str,
        f: impl FnOnce(&Recognizer) -> R,
    ) -> AppResult<R> {
        let model = self.store.resolve(chosen).ok_or_else(|| {
            AppError::Stt(
                "no speech model is installed; open Settings and download a model, or add your own"
                    .into(),
            )
        })?;
        let dir = model.dir.clone();
        let files = engine::detect(&dir).ok_or_else(|| {
            AppError::Stt(format!(
                "{} is incomplete or not a supported model; download it again in Settings, or pick another model",
                model.name
            ))
        })?;
        let gpu_blocked =
            settings.device != "cpu" && crate::gpu::provider() != "cpu" && files.cpu_only();
        let opts = EngineOptions {
            threads: self.hw.inference_threads(),
            language: if files.kind.takes_language() {
                whisper_language(&settings.language)
            } else {
                String::new()
            },
            provider: if settings.device == "cpu" || gpu_blocked {
                "cpu"
            } else {
                crate::gpu::provider()
            },
        };
        let key = format!("{}|{}", dir.display(), opts.provider);
        let entry = match self.find(&key) {
            Some(e) => e,
            None => {
                let _loading = self.load_lock.lock().unwrap_or_else(|p| p.into_inner());
                match self.find(&key) {
                    Some(e) => e, // another thread finished loading it meanwhile
                    None => {
                        let started = Instant::now();
                        let recognizer = engine::create(&files, &opts)?;
                        tracing::info!(
                            model = %model.id,
                            kind = files.kind.label(),
                            provider = opts.provider,
                            ms = started.elapsed().as_millis() as u64,
                            "speech model loaded"
                        );
                        let entry = Arc::new(Entry {
                            key,
                            info: Mutex::new(LoadedModel {
                                id: model.id.clone(),
                                name: model.name.clone(),
                                kind: files.kind.label(),
                                device: if opts.provider == "cpu" { "cpu" } else { "gpu" },
                                gpu_blocked,
                                language: opts.language.clone(),
                            }),
                            inner: Mutex::new(Loaded {
                                files: files.clone(),
                                recognizer,
                                language: opts.language.clone(),
                            }),
                        });
                        self.entries
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .push(entry.clone());
                        entry
                    }
                }
            }
        };
        let mut loaded = entry.inner.lock().unwrap_or_else(|p| p.into_inner());
        if loaded.language != opts.language {
            // Safe to reconfigure: holding the lock means nothing is decoding.
            engine::set_language(&loaded.recognizer, &loaded.files, &opts);
            tracing::info!(language = %opts.language, "speech model language switched");
            loaded.language = opts.language.clone();
            entry
                .info
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .language = opts.language.clone();
        }
        Ok(f(&loaded.recognizer))
    }
}

/// Removes recognizer artifacts: anything inside (), [], {} or <> (sound
/// captions such as "(crickets chirping)" or "[BLANK_AUDIO]"), music notes, and
/// the common hallucinations models produce on silence.
pub fn clean_transcript(text: &str) -> String {
    let t = strip_bracketed(text).replace(['♪', '♫'], " ");
    let mut t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    // A removed caption can leave "Hello , world"; reattach the punctuation.
    for p in [",", ".", "!", "?", ";", ":", "،", "؟"] {
        t = t.replace(&format!(" {p}"), p);
    }
    let t = t
        .trim_start_matches([',', '.', ';', ':', '،'])
        .trim()
        .to_string();
    const HALLUCINATIONS: &[&str] = &[
        "thank you.",
        "thanks for watching!",
        "thank you for watching.",
        "you",
        ".",
        "untertitel der amara.org-community",
        "untertitelung des zdf, 2020",
        "ترجمة نانسي قنقر",
        "اشتركوا في القناة",
    ];
    if HALLUCINATIONS.contains(&t.to_lowercase().as_str()) {
        return String::new();
    }
    t
}

/// Drops every bracket pair and what is between it, nested pairs included.
/// A bracket without its partner is kept, with the text after it.
fn strip_bracketed(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        match matching_close(&chars, i) {
            Some(end) => {
                out.push(' ');
                i = end + 1;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// Index of the bracket closing the one opened at `start`, if `start` opens one.
fn matching_close(chars: &[char], start: usize) -> Option<usize> {
    let close_of = |c: char| match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '<' => Some('>'),
        _ => None,
    };
    close_of(chars[start])?;
    let mut stack = Vec::new();
    for (i, &c) in chars.iter().enumerate().skip(start) {
        if let Some(close) = close_of(c) {
            stack.push(close);
        } else if stack.last() == Some(&c) {
            stack.pop();
            if stack.is_empty() {
                return Some(i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_artifacts() {
        assert_eq!(clean_transcript(" [BLANK_AUDIO] "), "");
        assert_eq!(clean_transcript("Thank you."), "");
        assert_eq!(clean_transcript("Hello   world [MUSIC]"), "Hello world");
        assert_eq!(clean_transcript("كيف حالك اليوم؟"), "كيف حالك اليوم؟");
        assert_eq!(
            clean_transcript("Hello (coughs), world [laughs] {x} <|nospeech|> ♪"),
            "Hello, world"
        );
        assert_eq!(
            clean_transcript("(outer (inner) still) open the file"),
            "open the file"
        );
        assert_eq!(clean_transcript("keep a < b as is"), "keep a < b as is");
    }

    #[test]
    fn language_setting_maps_to_whisper() {
        assert_eq!(whisper_language("auto"), "");
        assert_eq!(whisper_language(""), "");
        assert_eq!(whisper_language("AR"), "ar");
    }

    #[test]
    fn no_model_installed_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let svc = SttService::new(
            Arc::new(ModelStore::new(dir.path().into())),
            HardwareInfo::default(),
        );
        let err = svc
            .with_recognizer(&Settings::default(), |_| ())
            .map(|_| ())
            .unwrap_err();
        assert_eq!(err.code(), "stt_unavailable");
        assert!(!svc.is_ready(&Settings::default()));
    }
}
