//! Recognizer creation through sherpa-onnx. Whisper is the main family; models
//! the user adds can also be Moonshine, a transducer, or a single-file CTC /
//! SenseVoice / Paraformer / Dolphin / Omnilingual model.

use crate::errors::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// What a model folder holds, found out from its file names (see `detect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Whisper,
    /// encoder + merged decoder (the 2026 Moonshine release, `.ort` files).
    MoonshineV2,
    /// preprocess + encode + uncached/cached decode.
    MoonshineV1,
    /// encoder + decoder + joiner (Zipformer, NeMo transducer, ...).
    Transducer,
    NemoCtc,
    Dolphin,
    SenseVoice,
    Paraformer,
    Omnilingual,
    ZipformerCtc,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Whisper => "Whisper",
            Kind::MoonshineV2 | Kind::MoonshineV1 => "Moonshine",
            Kind::Transducer => "Transducer",
            Kind::NemoCtc => "NeMo CTC",
            Kind::Dolphin => "Dolphin",
            Kind::SenseVoice => "SenseVoice",
            Kind::Paraformer => "Paraformer",
            Kind::Omnilingual => "Omnilingual",
            Kind::ZipformerCtc => "Zipformer CTC",
        }
    }

    /// Only Whisper is told which language to decode.
    pub fn takes_language(self) -> bool {
        self == Kind::Whisper
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelFiles {
    pub kind: Kind,
    pub tokens: PathBuf,
    /// The one model file of single-file models.
    pub model: Option<PathBuf>,
    pub encoder: Option<PathBuf>,
    /// Decoder (Moonshine: the merged or the uncached one).
    pub decoder: Option<PathBuf>,
    pub joiner: Option<PathBuf>,
    /// Moonshine v1 only.
    pub preprocessor: Option<PathBuf>,
    pub cached_decoder: Option<PathBuf>,
}

impl ModelFiles {
    /// ONNX Runtime's `.ort` files are saved already optimized for the CPU
    /// (fused CPU-only operators); loading one on the CUDA provider makes the
    /// runtime abort the whole process, so those models always run on the CPU.
    pub fn cpu_only(&self) -> bool {
        [
            &self.model,
            &self.encoder,
            &self.decoder,
            &self.joiner,
            &self.preprocessor,
            &self.cached_decoder,
        ]
        .into_iter()
        .flatten()
        .any(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ort")))
    }
}

/// Inspects a model folder: which family it is and where its files are. The
/// int8 / quantized files are preferred when both exist. Single-file models
/// carry no hint in their files, so the folder's name decides (the names of the
/// sherpa-onnx downloads say what they are, e.g. `sherpa-onnx-dolphin-...`).
pub fn detect(dir: &Path) -> Option<ModelFiles> {
    let names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let tokens = names
        .iter()
        .find(|n| n.to_ascii_lowercase().ends_with("tokens.txt"))?;
    let is_model = |n: &&String| {
        let l = n.to_ascii_lowercase();
        l.ends_with(".onnx") || l.ends_with(".ort")
    };
    // The best file whose name has `keyword` and none of the `not` words.
    let pick = |keyword: &str, not: &[&str]| -> Option<PathBuf> {
        let mut found: Vec<&String> = names
            .iter()
            .filter(is_model)
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                l.contains(keyword) && !not.iter().any(|x| l.contains(x))
            })
            .collect();
        found.sort_by_key(|n| {
            let l = n.to_ascii_lowercase();
            (!l.contains(".int8."), !l.contains("quant"), l.len())
        });
        found.first().map(|n| dir.join(n.as_str()))
    };
    let files = |kind: Kind| ModelFiles {
        kind,
        tokens: dir.join(tokens),
        model: None,
        encoder: None,
        decoder: None,
        joiner: None,
        preprocessor: None,
        cached_decoder: None,
    };

    let (encoder, decoder, joiner) = (
        pick("encoder", &[]),
        pick("decoder", &[]),
        pick("joiner", &[]),
    );
    if let (Some(e), Some(d), Some(j)) = (&encoder, &decoder, &joiner) {
        return Some(ModelFiles {
            encoder: Some(e.clone()),
            decoder: Some(d.clone()),
            joiner: Some(j.clone()),
            ..files(Kind::Transducer)
        });
    }
    if let (Some(e), Some(d)) = (encoder, decoder) {
        let merged = d
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().contains("merged"));
        return Some(ModelFiles {
            encoder: Some(e),
            decoder: Some(d),
            ..files(if merged {
                Kind::MoonshineV2
            } else {
                Kind::Whisper
            })
        });
    }
    if let (Some(p), Some(e), Some(u), Some(c)) = (
        pick("preprocess", &[]),
        pick("encode", &[]),
        pick("uncached_decode", &[]),
        pick("cached_decode", &["uncached"]),
    ) {
        return Some(ModelFiles {
            preprocessor: Some(p),
            encoder: Some(e),
            decoder: Some(u),
            cached_decoder: Some(c),
            ..files(Kind::MoonshineV1)
        });
    }
    // One model file: the folder's name says which family.
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let has = |w: &str| folder.contains(w);
    let kind = if has("dolphin") {
        Kind::Dolphin
    } else if has("omnilingual") {
        Kind::Omnilingual
    } else if has("sense") {
        Kind::SenseVoice
    } else if has("paraformer") {
        Kind::Paraformer
    } else if has("zipformer") && has("ctc") {
        Kind::ZipformerCtc
    } else if has("nemo") || has("conformer") || has("citrinet") || has("ctc") {
        Kind::NemoCtc
    } else {
        return None;
    };
    let model = names
        .iter()
        .filter(is_model)
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            ![
                "encoder",
                "decoder",
                "joiner",
                "vad",
                "decode",
                "preprocess",
            ]
            .iter()
            .any(|x| l.contains(x))
        })
        .min_by_key(|n| {
            let l = n.to_ascii_lowercase();
            (!l.contains(".int8."), l.len())
        })?;
    Some(ModelFiles {
        model: Some(dir.join(model)),
        ..files(kind)
    })
}

fn s(p: &Path) -> Option<String> {
    Some(p.to_string_lossy().to_string())
}

/// A loaded recognizer (decodes complete utterances).
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
    /// "" lets the model detect the language (Whisper only).
    pub language: String,
    /// "cuda" | "cpu"
    pub provider: &'static str,
}

/// Switches a loaded Whisper model to another language ("" = detect) in place,
/// in well under a millisecond, instead of loading it again. The caller must
/// make sure nothing decodes on `rec` meanwhile. Does nothing for other families.
pub fn set_language(rec: &Recognizer, files: &ModelFiles, opts: &EngineOptions) {
    let (Some(encoder), Some(decoder)) = (&files.encoder, &files.decoder) else {
        return;
    };
    if !files.kind.takes_language() {
        return;
    }
    let text = |v: String| std::ffi::CString::new(v).unwrap_or_default();
    let encoder = text(encoder.to_string_lossy().to_string());
    let decoder = text(decoder.to_string_lossy().to_string());
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

pub fn create(files: &ModelFiles, opts: &EngineOptions) -> AppResult<Recognizer> {
    // Creating a recognizer proves the speech libraries loaded and ran.
    crate::gpu::mark_healthy();
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    config.model_config.tokens = s(&files.tokens);
    config.model_config.num_threads = opts.threads;
    config.model_config.provider = Some(opts.provider.into());
    config.decoding_method = Some("greedy_search".into());
    let p = |f: &Option<PathBuf>| f.as_deref().and_then(s);
    let m = &mut config.model_config;
    match files.kind {
        Kind::Whisper => {
            m.whisper = sherpa_onnx::OfflineWhisperModelConfig {
                encoder: p(&files.encoder),
                decoder: p(&files.decoder),
                language: Some(opts.language.clone()),
                task: Some("transcribe".into()),
                tail_paddings: -1,
                ..Default::default()
            }
        }
        Kind::MoonshineV2 => {
            m.moonshine = sherpa_onnx::OfflineMoonshineModelConfig {
                encoder: p(&files.encoder),
                merged_decoder: p(&files.decoder),
                ..Default::default()
            }
        }
        Kind::MoonshineV1 => {
            m.moonshine = sherpa_onnx::OfflineMoonshineModelConfig {
                preprocessor: p(&files.preprocessor),
                encoder: p(&files.encoder),
                uncached_decoder: p(&files.decoder),
                cached_decoder: p(&files.cached_decoder),
                ..Default::default()
            }
        }
        Kind::Transducer => {
            m.transducer = sherpa_onnx::OfflineTransducerModelConfig {
                encoder: p(&files.encoder),
                decoder: p(&files.decoder),
                joiner: p(&files.joiner),
            }
        }
        Kind::NemoCtc => {
            m.nemo_ctc = sherpa_onnx::OfflineNemoEncDecCtcModelConfig {
                model: p(&files.model),
            }
        }
        Kind::Dolphin => {
            m.dolphin = sherpa_onnx::OfflineDolphinModelConfig {
                model: p(&files.model),
            }
        }
        Kind::SenseVoice => {
            m.sense_voice = sherpa_onnx::OfflineSenseVoiceModelConfig {
                model: p(&files.model),
                language: None,
                use_itn: true,
            }
        }
        Kind::Paraformer => {
            m.paraformer = sherpa_onnx::OfflineParaformerModelConfig {
                model: p(&files.model),
            }
        }
        Kind::Omnilingual => {
            m.omnilingual = sherpa_onnx::OfflineOmnilingualAsrCtcModelConfig {
                model: p(&files.model),
            }
        }
        Kind::ZipformerCtc => {
            m.zipformer_ctc = sherpa_onnx::OfflineZipformerCtcModelConfig {
                model: p(&files.model),
            }
        }
    }
    sherpa_onnx::OfflineRecognizer::create(&config)
        .map(Recognizer)
        .ok_or_else(|| {
            AppError::Stt(
                "the speech model could not be loaded; check that its folder is complete, or switch to CPU"
                    .into(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, files: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for n in files {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        root
    }

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
        assert_eq!(f.kind, Kind::Whisper);
        assert!(f.encoder.unwrap().to_string_lossy().contains("int8"));
        assert!(f.decoder.unwrap().to_string_lossy().contains("int8"));
        let empty = tempfile::tempdir().unwrap();
        assert!(detect(empty.path()).is_none());
    }

    #[test]
    fn ort_files_are_cpu_only() {
        let v2 = folder(
            "moonshine-base-ar",
            &[
                "encoder_model.ort",
                "decoder_model_merged.ort",
                "tokens.txt",
            ],
        );
        assert!(detect(&v2.path().join("moonshine-base-ar"))
            .unwrap()
            .cpu_only());
        let w = folder("w", &["a-encoder.onnx", "a-decoder.onnx", "a-tokens.txt"]);
        assert!(!detect(&w.path().join("w")).unwrap().cpu_only());
    }

    #[test]
    fn tells_the_families_apart() {
        let v2 = folder(
            "moonshine-base-ar",
            &[
                "encoder_model.ort",
                "decoder_model_merged.ort",
                "tokens.txt",
            ],
        );
        let f = detect(&v2.path().join("moonshine-base-ar")).unwrap();
        assert_eq!(f.kind, Kind::MoonshineV2);

        let v1 = folder(
            "moonshine-tiny-en",
            &[
                "preprocess.onnx",
                "encode.int8.onnx",
                "uncached_decode.int8.onnx",
                "cached_decode.int8.onnx",
                "tokens.txt",
            ],
        );
        let f = detect(&v1.path().join("moonshine-tiny-en")).unwrap();
        assert_eq!(f.kind, Kind::MoonshineV1);
        assert!(f
            .cached_decoder
            .unwrap()
            .to_string_lossy()
            .contains("cached_decode.int8"));
        assert!(f
            .decoder
            .unwrap()
            .to_string_lossy()
            .contains("uncached_decode"));

        let tr = folder(
            "zipformer-en",
            &["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"],
        );
        assert_eq!(
            detect(&tr.path().join("zipformer-en")).unwrap().kind,
            Kind::Transducer
        );
    }

    #[test]
    fn single_file_models_are_known_by_their_folder_name() {
        let d = folder(
            "sherpa-onnx-dolphin-base-ctc-multi-lang-int8",
            &["model.int8.onnx", "tokens.txt"],
        );
        let f = detect(
            &d.path()
                .join("sherpa-onnx-dolphin-base-ctc-multi-lang-int8"),
        )
        .unwrap();
        assert_eq!(f.kind, Kind::Dolphin);
        assert!(f.model.is_some());
        // Nothing says what a plainly named single-file folder is.
        let unknown = folder("my-model", &["model.onnx", "tokens.txt"]);
        assert!(detect(&unknown.path().join("my-model")).is_none());
        // No tokens file, no model.
        let bare = folder("whisper", &["encoder.onnx", "decoder.onnx"]);
        assert!(detect(&bare.path().join("whisper")).is_none());
    }

    /// Loads the model folder named by `LA_TEST_MODEL_DIR` (on the GPU when
    /// `LA_TEST_PROVIDER=cuda`) and decodes `LA_TEST_WAV` (16 kHz, 16-bit mono)
    /// or a second of silence: proves a real download is detected and runs.
    #[test]
    #[ignore = "needs a model folder: set LA_TEST_MODEL_DIR"]
    fn loads_a_real_model_folder() {
        let dir = std::path::PathBuf::from(
            std::env::var("LA_TEST_MODEL_DIR").expect("set LA_TEST_MODEL_DIR"),
        );
        let files = detect(&dir).expect("model folder not recognised");
        println!("kind: {}", files.kind.label());
        let provider = if std::env::var("LA_TEST_PROVIDER").as_deref() == Ok("cuda") {
            "cuda"
        } else {
            "cpu"
        };
        let opts = EngineOptions {
            threads: 2,
            language: String::new(),
            provider,
        };
        let started = std::time::Instant::now();
        let rec = create(&files, &opts).expect("model did not load");
        println!("{provider}: loaded in {:?}", started.elapsed());
        let samples: Vec<f32> = match std::env::var("LA_TEST_WAV") {
            Ok(wav) => {
                let bytes = std::fs::read(wav).unwrap();
                let (pairs, _) = bytes[44..].as_chunks::<2>();
                pairs
                    .iter()
                    .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
                    .collect()
            }
            Err(_) => vec![0.0; 16_000],
        };
        for round in 0..3 {
            let t = std::time::Instant::now();
            let text = rec.transcribe(&samples);
            println!("run {round}: {:?} in {:?}", text, t.elapsed());
        }
    }
}
