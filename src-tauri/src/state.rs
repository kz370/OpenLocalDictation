//! Application state shared by commands, tray and shortcut handlers.

use crate::hardware::HardwareInfo;
use crate::models::ModelStore;
use crate::settings::SettingsStore;
use crate::stt::session::VoiceSessions;
use crate::stt::SttService;
use crate::typing::LiveTyper;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
}

/// A finished dictation waiting in the overlay for the user to edit and confirm.
#[derive(Debug, Clone)]
pub struct ReviewPending {
    pub raw: String,
}

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Arc<SettingsStore>,
    pub models: Arc<ModelStore>,
    pub stt: Arc<SttService>,
    pub voice: Arc<VoiceSessions>,
    pub hardware: HardwareInfo,
    pub downloads: Mutex<HashMap<String, CancellationToken>>,
    pub dictation_busy: AtomicBool,
    /// Set by Esc/X cancel; consumed by `run_dictation` before any insert.
    pub dictation_cancel: AtomicBool,
    /// Identifies dictation sessions so a delayed hide from an older take
    /// cannot dismiss a newer session's overlay.
    pub dictation_session_generation: AtomicU64,
    /// Tracks what has been live-typed so far (insert method "type", review off).
    pub live_typer: LiveTyper,
    /// A finished dictation held in the overlay for editing.
    pub dictation_review: Mutex<Option<ReviewPending>>,
    /// Window that had focus when dictation started (native handle, 0 = none).
    pub dictation_target: AtomicIsize,
    /// Review shortcuts temporarily replace the dictation hotkey while the
    /// editor is open, so they work even though another app has focus.
    pub review_shortcuts_active: AtomicBool,
    pub shortcut_errors: Mutex<Vec<String>>,
}
