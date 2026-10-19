//! Typed errors per subsystem, and `AppError`, the single error type returned to the
//! frontend (serialized as a plain user-readable string).

use serde::{Serialize, Serializer};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid config file {path}: {message}")]
    Parse { path: String, message: String },
    #[error("invalid Twitch channel name {0:?}: use 1-25 letters, digits or underscores")]
    Channel(String),
    #[error("notification sound {path:?}: {reason}")]
    Sound { path: String, reason: String },
    #[error("{field} must be between {min} and {max}")]
    Range {
        field: &'static str,
        min: f64,
        max: f64,
    },
}

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("engine unavailable: {0}")]
    Unavailable(String),
    #[error("voice model not found: {}", .0.display())]
    ModelMissing(PathBuf),
    #[error("synthesis failed: {0}")]
    Failed(String),
    #[error("synthesis timed out")]
    Timeout,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no usable audio output device: {0}")]
    Device(String),
    #[error("cannot use sound file {path:?}: {reason}")]
    Sound { path: String, reason: String },
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Tts(#[from] TtsError),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error("{0}")]
    Msg(String),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Msg(s.into())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
