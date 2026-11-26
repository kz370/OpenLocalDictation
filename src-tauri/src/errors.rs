//! Application error type. Every error carries a stable `code` the UI maps to
//! a message that says what went wrong, why and how to fix it.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Local speech recognition unavailable: {0}")]
    Stt(String),
    #[error("Audio device error: {0}")]
    Audio(String),
    #[error("Model download failed: {0}")]
    Download(String),
    #[error("Invalid input: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Stt(_) => "stt_unavailable",
            AppError::Audio(_) => "audio",
            AppError::Download(_) => "download",
            AppError::Invalid(_) => "invalid",
            AppError::Io(_) => "io",
            AppError::Cancelled => "cancelled",
            AppError::Other(_) => "other",
        }
    }
}

#[derive(Serialize)]
struct WireError<'a> {
    code: &'a str,
    detail: String,
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WireError {
            code: self.code(),
            detail: self.to_string(),
        }
        .serialize(serializer)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Invalid(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Other(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
