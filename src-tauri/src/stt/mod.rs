//! Local speech-to-text with Whisper. Audio never leaves the machine.

pub mod engine;
pub mod session;

use crate::errors::{AppError, AppResult};
use crate::hardware::HardwareInfo;
use crate::models::ModelStore;
use crate::settings::Settings;
use engine::{EngineOptions, Recognizer, WhisperFiles};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Loaded {
    key: String,
    files: WhisperFiles,
    recognizer: Recognizer,
    /// Language the recognizer currently decodes.
    language: String,
}

pub struct SttService {
    store: Arc<ModelStore>,
    hw: HardwareInfo,
    loaded: Mutex<Option<Loaded>>,
    loaded_id: Mutex<Option<String>>,
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
            loaded: Mutex::new(None),
            loaded_id: Mutex::new(None),
        }
    }

    pub fn is_ready(&self, settings: &Settings) -> bool {
        self.store.resolve(settings.effective_model()).is_some()
    }

    pub fn loaded_model(&self) -> Option<String> {
        self.loaded_id
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Drops the loaded recognizer (model or device changed, model deleted).
    pub fn unload(&self) {
        *self.loaded.lock().unwrap_or_else(|p| p.into_inner()) = None;
        *self.loaded_id.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Runs `f` with the loaded recognizer, loading it first if needed. The
    /// recognizer stays loaded for the next call; a language change switches
    /// it in place instead of reloading.
    pub fn with_recognizer<R>(
        &self,
        settings: &Settings,
        f: impl FnOnce(&Recognizer) -> R,
    ) -> AppResult<R> {
        let model = self.store.resolve(settings.effective_model()).ok_or_else(|| {
            AppError::Stt(
                "no speech model is installed; open Settings and download Whisper tiny, base or small"
                    .into(),
            )
        })?;
        let dir = model.dir.clone();
        let files = engine::detect(&dir).ok_or_else(|| {
            AppError::Stt(format!(
                "{} is incomplete; delete it in Settings and download it again",
                model.name
            ))
        })?;
        let opts = EngineOptions {
            threads: self.hw.inference_threads(),
            language: whisper_language(&settings.language),
            provider: if settings.device == "cpu" {
                "cpu"
            } else {
                crate::gpu::provider()
            },
        };
        let key = format!("{}|{}", dir.display(), opts.provider);
        let mut guard = self.loaded.lock().unwrap_or_else(|p| p.into_inner());
        if guard.as_ref().map(|l| l.key != key).unwrap_or(true) {
            *guard = None; // free the previous model before loading another
            *self.loaded_id.lock().unwrap_or_else(|p| p.into_inner()) = None;
            let started = Instant::now();
            let recognizer = engine::create(&files, &opts)?;
            tracing::info!(
                model = %model.id,
                provider = opts.provider,
                ms = started.elapsed().as_millis() as u64,
                "speech model loaded"
            );
            *guard = Some(Loaded {
                key,
                files: files.clone(),
                recognizer,
                language: opts.language.clone(),
            });
            *self.loaded_id.lock().unwrap_or_else(|p| p.into_inner()) = Some(model.id.clone());
        }
        let loaded = guard.as_mut().expect("recognizer loaded");
        if loaded.language != opts.language {
            // Safe to reconfigure: holding the lock means nothing is decoding.
            engine::set_language(&loaded.recognizer, &loaded.files, &opts);
            tracing::info!(language = %opts.language, "speech model language switched");
            loaded.language = opts.language.clone();
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
