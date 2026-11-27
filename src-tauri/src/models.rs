//! The three Whisper models (tiny, base, small), their consented downloads
//! with SHA-256 verification, and the Silero VAD that ships inside the exe.

use crate::errors::{AppError, AppResult};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const COMPLETE_MARKER: &str = ".complete";
/// Silero VAD ships inside the exe (~630 KB) instead of being downloaded.
const BUNDLED_VAD: &[u8] = include_bytes!("../assets/silero_vad.onnx");
const HF: &str = "https://huggingface.co/csukuangfj";
const TOKENS_SHA: &str = "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126";

pub struct RemoteFile {
    pub name: &'static str,
    pub dest: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
}

pub struct CatalogModel {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    pub repo: &'static str,
    pub prefix: &'static str,
    pub files: &'static [RemoteFile],
}

impl CatalogModel {
    pub fn download_size(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }

    fn url(&self, file: &RemoteFile) -> String {
        format!(
            "{HF}/{}/resolve/main/{}{}",
            self.repo, self.prefix, file.name
        )
    }
}

const fn tokens() -> RemoteFile {
    RemoteFile {
        name: "-tokens.txt",
        dest: "tokens.txt",
        sha256: TOKENS_SHA,
        size_bytes: 816_730,
    }
}

/// Smallest first. Quality and cost both rise with the size.
pub static CATALOG: &[CatalogModel] = &[
    CatalogModel {
        id: "whisper-tiny",
        name: "Whisper tiny",
        blurb: "Fastest, lowest accuracy. Fine for short English phrases.",
        repo: "sherpa-onnx-whisper-tiny",
        prefix: "tiny",
        files: &[
            RemoteFile {
                name: "-encoder.int8.onnx",
                dest: "encoder.int8.onnx",
                sha256: "d24fb083ae3b1041fc24e97971d60e280c9342201fbb67b0ab428a8b4a51a434",
                size_bytes: 12_937_772,
            },
            RemoteFile {
                name: "-decoder.int8.onnx",
                dest: "decoder.int8.onnx",
                sha256: "d2fece8dd42771f1df975c6c0445770d0c292bf7547c2cae04a6c0cc57540925",
                size_bytes: 89_855_401,
            },
            tokens(),
        ],
    },
    CatalogModel {
        id: "whisper-base",
        name: "Whisper base",
        blurb: "Good balance of speed and accuracy on a CPU.",
        repo: "sherpa-onnx-whisper-base",
        prefix: "base",
        files: &[
            RemoteFile {
                name: "-encoder.int8.onnx",
                dest: "encoder.int8.onnx",
                sha256: "0b8fb1304b6109976038efff5ace81720e00386f3ff6b54ee8c75291ca0a1e11",
                size_bytes: 29_120_534,
            },
            RemoteFile {
                name: "-decoder.int8.onnx",
                dest: "decoder.int8.onnx",
                sha256: "9759d217388a01b3a4c7c15533201067b48ae819c4daafc8624e64b9409dc02d",
                size_bytes: 130_672_026,
            },
            tokens(),
        ],
    },
    CatalogModel {
        id: "whisper-small",
        name: "Whisper small",
        blurb: "Best accuracy, including Arabic and German. Best on a GPU.",
        repo: "sherpa-onnx-whisper-small",
        prefix: "small",
        files: &[
            RemoteFile {
                name: "-encoder.int8.onnx",
                dest: "encoder.int8.onnx",
                sha256: "4cbe7b22fa9026b843b60a68640c747de05bafb1a11b57edc0e66c232d9f33a9",
                size_bytes: 112_442_483,
            },
            RemoteFile {
                name: "-decoder.int8.onnx",
                dest: "decoder.int8.onnx",
                sha256: "acad50b5c782696e91b55914cc5ab4f756f1532f76e22aa6fc615f39fb69a8ee",
                size_bytes: 262_226_114,
            },
            tokens(),
        ],
    },
    CatalogModel {
        id: "whisper-turbo",
        name: "Whisper large-v3 turbo",
        blurb: "Near large-v3 accuracy at a fraction of the decoding cost. Best on a GPU.",
        repo: "sherpa-onnx-whisper-turbo",
        prefix: "turbo",
        files: &[
            RemoteFile {
                name: "-encoder.int8.onnx",
                dest: "encoder.int8.onnx",
                sha256: "b02dcdf54f348741e93fe732b67d933c8dcb6735655f710640143081db38878b",
                size_bytes: 674_716_297,
            },
            RemoteFile {
                name: "-decoder.int8.onnx",
                dest: "decoder.int8.onnx",
                sha256: "20accd02388482eb3a46bd615631adfdc85e1eb2c7db9ea3f02a40ffe6b81547",
                size_bytes: 361_080_764,
            },
            tokens(),
        ],
    },
];

pub fn find(id: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| m.id == id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub blurb: String,
    pub download_bytes: u64,
    pub installed: bool,
    pub downloading: bool,
    /// A model found in the models folder that is not in the catalog.
    pub custom: bool,
    pub path: Option<String>,
    /// Model family: "Whisper", "Moonshine", ...
    pub kind: String,
}

/// The model a dictation session will use.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
}

/// A model folder this app did not download: one found in the models folder,
/// or one the user added by hand.
#[derive(Debug, Clone)]
pub struct CustomModel {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
    pub kind: &'static str,
    /// Added with "Add your own model" (not found by scanning).
    pub added: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    /// "downloading" | "verifying" | "done" | "error" | "cancelled"
    pub state: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

pub struct ModelStore {
    /// Folder holding the models; the user can point it at one shared with
    /// other apps. Catalog models live in `<root>/stt/<id>`.
    root: std::sync::RwLock<PathBuf>,
    /// Folders the user added by hand ("Add your own model").
    user: std::sync::RwLock<Vec<PathBuf>>,
    /// Where the embedded VAD model is written (always the app's own folder).
    vad_dir: PathBuf,
}

impl ModelStore {
    pub fn new(root: PathBuf) -> Self {
        let vad_dir = root.join("vad");
        Self::with_vad_dir(root, vad_dir)
    }

    pub fn with_vad_dir(root: PathBuf, vad_dir: PathBuf) -> Self {
        Self {
            root: std::sync::RwLock::new(root),
            user: std::sync::RwLock::new(Vec::new()),
            vad_dir,
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set_user_dirs(&self, dirs: Vec<PathBuf>) {
        *self.user.write().unwrap_or_else(|p| p.into_inner()) = dirs;
    }

    pub fn set_root(&self, root: PathBuf) {
        *self.root.write().unwrap_or_else(|p| p.into_inner()) = root;
    }

    /// Where a catalog model is downloaded to: `<root>/stt/<id>`, or
    /// `<root>/<id>` when the chosen folder is itself the `stt` folder.
    pub fn model_dir(&self, id: &str) -> PathBuf {
        let root = self.root();
        if root
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("stt"))
        {
            root.join(id)
        } else {
            root.join("stt").join(id)
        }
    }

    /// Where a catalog model is installed, if it is: either layout counts, and
    /// so does a folder with the right Whisper files that another app put there.
    pub fn installed_dir(&self, id: &str) -> Option<PathBuf> {
        let root = self.root();
        [root.join("stt").join(id), root.join(id)]
            .into_iter()
            .find(|d| d.join(COMPLETE_MARKER).is_file() || is_whisper_folder(d))
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.installed_dir(id).is_some()
    }

    /// Installed catalog models, smallest first.
    pub fn installed_ids(&self) -> Vec<&'static str> {
        CATALOG
            .iter()
            .filter(|m| self.is_installed(m.id))
            .map(|m| m.id)
            .collect()
    }

    /// Model folders that are not catalog installs: the ones in the models
    /// folder (and its `stt` sub-folder), put there by the user or another app,
    /// then the ones the user added by hand.
    pub fn custom_models(&self) -> Vec<CustomModel> {
        let root = self.root();
        let mut out: Vec<CustomModel> = Vec::new();
        for base in [root.clone(), root.join("stt")] {
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            for e in entries.flatten() {
                let dir = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !dir.is_dir() || name.starts_with('.') || find(&name).is_some() {
                    continue;
                }
                let Some(files) = crate::stt::engine::detect(&dir) else {
                    continue;
                };
                if out.iter().any(|m| m.dir == dir) {
                    continue;
                }
                out.push(CustomModel {
                    id: format!("custom:{}", dir.display()),
                    name,
                    dir,
                    kind: files.kind.label(),
                    added: false,
                });
            }
        }
        out.sort_by_key(|m| m.name.to_lowercase());
        let added = self.user.read().unwrap_or_else(|p| p.into_inner()).clone();
        for dir in added {
            let Some(files) = crate::stt::engine::detect(&dir) else {
                continue; // moved or deleted since it was added
            };
            if out.iter().any(|m| m.dir == dir) {
                continue;
            }
            out.push(CustomModel {
                id: format!("custom:{}", dir.display()),
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir.display().to_string()),
                dir,
                kind: files.kind.label(),
                added: true,
            });
        }
        out
    }

    /// The chosen model when present, otherwise the best one available: the
    /// largest installed catalog model, then any custom model.
    pub fn resolve(&self, chosen: &str) -> Option<Resolved> {
        if let Some((m, dir)) = find(chosen).and_then(|m| Some((m, self.installed_dir(m.id)?))) {
            return Some(Resolved {
                id: m.id.into(),
                name: m.name.into(),
                dir,
            });
        }
        let custom = self.custom_models();
        if let Some(c) = custom.iter().find(|c| c.id == chosen) {
            return Some(Resolved {
                id: c.id.clone(),
                name: c.name.clone(),
                dir: c.dir.clone(),
            });
        }
        if let Some(id) = self.installed_ids().last() {
            let m = find(id)?;
            return Some(Resolved {
                id: m.id.into(),
                name: m.name.into(),
                dir: self.installed_dir(m.id)?,
            });
        }
        custom.into_iter().next().map(|c| Resolved {
            id: c.id,
            name: c.name,
            dir: c.dir,
        })
    }

    /// Writes the bundled VAD model out so the recognizer library can open it.
    pub fn install_bundled_vad(&self) -> AppResult<PathBuf> {
        let path = self.vad_dir.join("silero_vad.onnx");
        if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(BUNDLED_VAD.len() as u64) {
            std::fs::create_dir_all(&self.vad_dir)?;
            std::fs::write(&path, BUNDLED_VAD)?;
        }
        Ok(path)
    }

    pub async fn install(
        &self,
        model: &CatalogModel,
        cancel: CancellationToken,
        on_progress: &(dyn Fn(DownloadProgress) + Send + Sync),
    ) -> AppResult<()> {
        let root = self.root();
        std::fs::create_dir_all(&root)?;
        let dir = self.model_dir(model.id);
        let staging = root.join(format!(".staging-{}", model.id));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;

        let total = model.download_size();
        let mut done_bytes = 0u64;
        let progress = |state: &str, downloaded: u64, error: Option<String>| {
            on_progress(DownloadProgress {
                model_id: model.id.into(),
                state: state.into(),
                downloaded_bytes: downloaded,
                total_bytes: total,
                error,
            })
        };
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .user_agent(concat!("LocalDictation/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Download(e.to_string()))?;

        let result: AppResult<()> = async {
            for file in model.files {
                let url = model.url(file);
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| AppError::Download(e.to_string()))?;
                if !resp.status().is_success() {
                    return Err(AppError::Download(format!(
                        "{url} returned {}",
                        resp.status()
                    )));
                }
                let dest = safe_join(&staging, file.dest)?;
                let mut out = std::fs::File::create(&dest)?;
                let mut hasher = Sha256::new();
                let mut stream = resp.bytes_stream();
                let mut file_bytes = 0u64;
                let mut last_emit = Instant::now();
                loop {
                    let next = tokio::select! {
                        n = stream.next() => n,
                        _ = cancel.cancelled() => return Err(AppError::Cancelled),
                    };
                    let Some(chunk) = next else { break };
                    let chunk = chunk.map_err(|e| AppError::Download(e.to_string()))?;
                    out.write_all(&chunk)?;
                    hasher.update(&chunk);
                    file_bytes += chunk.len() as u64;
                    if last_emit.elapsed() > Duration::from_millis(250) {
                        progress("downloading", done_bytes + file_bytes, None);
                        last_emit = Instant::now();
                    }
                }
                out.flush()?;
                drop(out);
                done_bytes += file_bytes;
                progress("verifying", done_bytes, None);
                let actual = hex::encode(hasher.finalize());
                if !actual.eq_ignore_ascii_case(file.sha256) {
                    return Err(AppError::Download(format!(
                        "checksum mismatch for {url}; the file was not installed, try again"
                    )));
                }
            }
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                let _ = std::fs::remove_dir_all(&dir);
                if let Some(parent) = dir.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&staging, &dir)?;
                std::fs::write(
                    dir.join(COMPLETE_MARKER),
                    serde_json::json!({"id": model.id, "installedAt": chrono::Utc::now().to_rfc3339()})
                        .to_string(),
                )?;
                tracing::info!(model = model.id, bytes = done_bytes, "model installed");
                progress("done", total, None);
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                if matches!(e, AppError::Cancelled) {
                    progress("cancelled", done_bytes, None);
                } else {
                    tracing::warn!(model = model.id, error = %e, "model download failed");
                    progress("error", done_bytes, Some(e.to_string()));
                }
                Err(e)
            }
        }
    }
}

/// True for a folder holding Whisper files (encoder + decoder + tokens, no
/// joiner, which would make it a transducer this app cannot run).
fn is_whisper_folder(dir: &Path) -> bool {
    crate::stt::engine::detect(dir).is_some_and(|f| f.kind == crate::stt::engine::Kind::Whisper)
}

/// Joins a plain file name onto `root`, rejecting anything that could escape it.
fn safe_join(root: &Path, rel: &str) -> AppResult<PathBuf> {
    if rel.is_empty() || rel.contains(['/', '\\']) || rel == ".." {
        return Err(AppError::Download(format!("unsafe file name: {rel}")));
    }
    Ok(root.join(rel))
}

/// Extracts a .tar.bz2 into `dest`, stripping the archive's top-level folder.
/// Used by the GPU pack.
pub fn extract_tar_bz2(archive: &Path, dest: &Path) -> AppResult<()> {
    use std::path::Component;
    let f = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(f)));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            continue;
        }
        let mut target = dest.to_path_buf();
        for c in stripped.components() {
            match c {
                Component::Normal(p) => target.push(p),
                _ => {
                    return Err(AppError::Download(format!(
                        "unsafe path in archive: {}",
                        stripped.display()
                    )))
                }
            }
        }
        if kind.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(&target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        assert_eq!(CATALOG.len(), 4);
        for m in CATALOG {
            for f in m.files {
                assert_eq!(f.sha256.len(), 64);
            }
            assert!(m.url(&m.files[0]).starts_with("https://huggingface.co/"));
        }
        let tiny = find("whisper-tiny").unwrap();
        assert_eq!(
            tiny.url(&tiny.files[2]),
            "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny/resolve/main/tiny-tokens.txt"
        );
    }

    #[test]
    fn resolve_prefers_choice_then_largest_installed() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path().to_path_buf());
        assert!(store.resolve("").is_none());
        for id in ["whisper-tiny", "whisper-base"] {
            std::fs::create_dir_all(store.model_dir(id)).unwrap();
            std::fs::write(store.model_dir(id).join(COMPLETE_MARKER), b"1").unwrap();
        }
        assert_eq!(store.resolve("").unwrap().id, "whisper-base");
        assert_eq!(store.resolve("whisper-tiny").unwrap().id, "whisper-tiny");
        assert_eq!(store.resolve("whisper-small").unwrap().id, "whisper-base");
    }

    #[test]
    fn finds_whisper_folders_the_user_already_has() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path().to_path_buf());
        let mk = |base: &Path, name: &str, files: &[&str]| {
            let d = base.join(name);
            std::fs::create_dir_all(&d).unwrap();
            for f in files {
                std::fs::write(d.join(f), b"x").unwrap();
            }
        };
        let whisper = ["a-encoder.onnx", "a-decoder.onnx", "a-tokens.txt"];
        mk(dir.path(), "my-whisper", &whisper);
        mk(&dir.path().join("stt"), "other-whisper", &whisper);
        mk(
            dir.path(),
            "parakeet",
            &["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"],
        );
        mk(dir.path(), "notes", &["readme.txt"]);
        let found: Vec<String> = store.custom_models().into_iter().map(|m| m.name).collect();
        // A transducer folder is a model the app can run too; "notes" is not one.
        assert_eq!(found, vec!["my-whisper", "other-whisper", "parakeet"]);
        // A folder added by hand is listed (once) after the scanned ones.
        let own = tempfile::tempdir().unwrap();
        mk(
            own.path(),
            "sherpa-onnx-moonshine-base-ar",
            &[
                "encoder_model.ort",
                "decoder_model_merged.ort",
                "tokens.txt",
            ],
        );
        let moon = own.path().join("sherpa-onnx-moonshine-base-ar");
        store.set_user_dirs(vec![moon.clone(), moon.clone(), dir.path().join("gone")]);
        let all = store.custom_models();
        let added: Vec<_> = all.iter().filter(|m| m.added).collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].kind, "Moonshine");
        store.set_user_dirs(Vec::new());
        // With nothing downloaded a custom model is picked; a chosen one wins.
        let first = store.resolve("").unwrap();
        assert_eq!(first.name, "my-whisper");
        let pick = store.resolve(&store.custom_models()[1].id).unwrap();
        assert_eq!(pick.name, "other-whisper");
        // The folder can be pointed somewhere else.
        let other = tempfile::tempdir().unwrap();
        store.set_root(other.path().to_path_buf());
        assert!(store.resolve("").is_none());
    }

    #[test]
    fn rejects_traversal() {
        assert!(safe_join(Path::new("C:/m"), "../x").is_err());
        assert!(safe_join(Path::new("C:/m"), "a/b").is_err());
        assert!(safe_join(Path::new("C:/m"), "tokens.txt").is_ok());
    }

    #[test]
    fn bundled_vad_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::new(dir.path().to_path_buf());
        let p = store.install_bundled_vad().unwrap();
        assert_eq!(
            std::fs::metadata(&p).unwrap().len(),
            BUNDLED_VAD.len() as u64
        );
        assert_eq!(store.install_bundled_vad().unwrap(), p);
    }

    #[test]
    fn works_when_the_chosen_folder_is_the_stt_folder() {
        let root = tempfile::tempdir().unwrap();
        let stt = root.path().join("stt");
        for (name, files) in [
            (
                "whisper-base",
                [
                    "base-encoder.int8.onnx",
                    "base-decoder.int8.onnx",
                    "base-tokens.txt",
                ],
            ),
            (
                "whisper-custom",
                ["encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"],
            ),
        ] {
            std::fs::create_dir_all(stt.join(name)).unwrap();
            for f in files {
                std::fs::write(stt.join(name).join(f), b"x").unwrap();
            }
        }
        for chosen in [stt.clone(), root.path().to_path_buf()] {
            let store = ModelStore::new(chosen);
            assert!(
                store.is_installed("whisper-base"),
                "catalog model without a marker"
            );
            assert!(!store.is_installed("whisper-tiny"));
            let custom: Vec<String> = store.custom_models().into_iter().map(|m| m.name).collect();
            assert_eq!(custom, vec!["whisper-custom"]);
            assert_eq!(store.resolve("").unwrap().id, "whisper-base");
        }
        // A download into the stt folder itself does not nest another stt/ folder.
        assert_eq!(
            ModelStore::new(stt.clone()).model_dir("whisper-tiny"),
            stt.join("whisper-tiny")
        );
    }
}
