//! Whisper recognizer creation through sherpa-onnx.

use crate::errors::{AppError, AppResult};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct WhisperFiles {
    pub tokens: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
}

/// Inspects a model folder for the int8 (or plain) Whisper files.
pub fn detect(dir: &Path) -> Option<WhisperFiles> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let pick = |keyword: &str| -> Option<PathBuf> {
        let matching: Vec<&String> = names
            .iter()
            .filter(|n| n.ends_with(".onnx") && n.to_ascii_lowercase().contains(keyword))
            .collect();
        let best = matching
            .iter()
            .find(|n| n.contains(".int8."))
            .or_else(|| matching.first())?;
        Some(dir.join(best.as_str()))
    };
    let tokens = names.iter().find(|n| n.ends_with("tokens.txt"))?;
    Some(WhisperFiles {
        tokens: dir.join(tokens),
        encoder: pick("encoder")?,
        decoder: pick("decoder")?,
    })
}

fn s(p: &Path) -> Option<String> {
    Some(p.to_string_lossy().to_string())
}

/// A loaded Whisper recognizer (decodes complete utterances).
pub struct Recognizer(sherpa_onnx::OfflineRecognizer);

impl Recognizer {
    /// Decodes one utterance (16 kHz mono).
    pub fn transcribe(&self, samples: &[f32]) -> String {
        let stream = self.0.create_stream();
        stream.accept_waveform(16_000, samples);
        self.0.decode(&stream);
        stream.get_result().map(|x| x.text).unwrap_or_default()
    }
}

extern "C" {
    // Exported by the sherpa-onnx C library but not bound by the Rust crates.
    fn SherpaOnnxOfflineRecognizerSetConfig(
        recognizer: *const sherpa_onnx_sys::OfflineRecognizer,
        config: *const sherpa_onnx_sys::OfflineRecognizerConfig,
    );
}

// `set_language` reads the wrapper's private C pointer, which is its only
// field; fail the build if the wrapper ever grows.
const _: () = assert!(
    std::mem::size_of::<sherpa_onnx::OfflineRecognizer>()
        == std::mem::size_of::<*const sherpa_onnx_sys::OfflineRecognizer>()
);

pub struct EngineOptions {
    pub threads: i32,
    /// "" lets the model detect the language.
    pub language: String,
    /// "cuda" | "cpu"
    pub provider: &'static str,
}

/// Switches a loaded model to another language ("" = detect) in place, in well
/// under a millisecond, instead of loading it again. The caller must make
/// sure nothing decodes on `rec` meanwhile.
pub fn set_language(rec: &Recognizer, files: &WhisperFiles, opts: &EngineOptions) {
    let text = |v: String| std::ffi::CString::new(v).unwrap_or_default();
    let encoder = text(files.encoder.to_string_lossy().to_string());
    let decoder = text(files.decoder.to_string_lossy().to_string());
    let tokens = text(files.tokens.to_string_lossy().to_string());
    let language = text(opts.language.clone());
    let task = text("transcribe".into());
    let provider = text(opts.provider.into());
    let method = text("greedy_search".into());
    // SAFETY: the C API reads an all-zero config as "defaults", and every
    // string points into a CString that outlives the call. The pointer is the
    // wrapper's only field (checked above) and lives as long as `rec`.
    unsafe {
        let mut c: sherpa_onnx_sys::OfflineRecognizerConfig = std::mem::zeroed();
        c.model_config.whisper.encoder = encoder.as_ptr();
        c.model_config.whisper.decoder = decoder.as_ptr();
        c.model_config.whisper.language = language.as_ptr();
        c.model_config.whisper.task = task.as_ptr();
        c.model_config.whisper.tail_paddings = -1;
        c.model_config.tokens = tokens.as_ptr();
        c.model_config.num_threads = opts.threads;
        c.model_config.provider = provider.as_ptr();
        c.decoding_method = method.as_ptr();
        let ptr: *const sherpa_onnx_sys::OfflineRecognizer = std::mem::transmute_copy(&rec.0);
        SherpaOnnxOfflineRecognizerSetConfig(ptr, &c);
    }
}

pub fn create(files: &WhisperFiles, opts: &EngineOptions) -> AppResult<Recognizer> {
    // Creating a recognizer proves the speech libraries loaded and ran.
    crate::gpu::mark_healthy();
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    config.model_config.tokens = s(&files.tokens);
    config.model_config.num_threads = opts.threads;
    config.model_config.provider = Some(opts.provider.into());
    config.decoding_method = Some("greedy_search".into());
    config.model_config.whisper = sherpa_onnx::OfflineWhisperModelConfig {
        encoder: s(&files.encoder),
        decoder: s(&files.decoder),
        language: Some(opts.language.clone()),
        task: Some("transcribe".into()),
        tail_paddings: -1,
        ..Default::default()
    };
    sherpa_onnx::OfflineRecognizer::create(&config)
        .map(Recognizer)
        .ok_or_else(|| {
            AppError::Stt(
                "the speech model could not be loaded; reinstall it in Settings, or switch to CPU"
                    .into(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_prefers_int8() {
        let dir = tempfile::tempdir().unwrap();
        for n in [
            "encoder.onnx",
            "encoder.int8.onnx",
            "decoder.onnx",
            "decoder.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let f = detect(dir.path()).unwrap();
        assert!(f.encoder.to_string_lossy().contains("int8"));
        assert!(f.decoder.to_string_lossy().contains("int8"));
        let empty = tempfile::tempdir().unwrap();
        assert!(detect(empty.path()).is_none());
    }
}
