//! Backend → frontend events. Core modules emit through [`Emit`] so they stay independent of Tauri.

use serde::Serialize;
use std::sync::Arc;

pub const EVT_CHAT_MESSAGE: &str = "chat:message";
pub const EVT_CHAT_STATUS: &str = "chat:status";
pub const EVT_QUEUE_UPDATED: &str = "tts:queue-updated";
pub const EVT_TTS_ERROR: &str = "tts:error";
pub const EVT_AUDIO_ERROR: &str = "audio:error";
pub const EVT_SETTINGS_UPDATED: &str = "settings:updated";
pub const EVT_MODELS_PROGRESS: &str = "models:progress";
pub const EVT_MODELS_CHANGED: &str = "models:changed";
pub const EVT_RUNTIME_PROGRESS: &str = "runtime:progress";

pub type Emit = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

pub fn emit<T: Serialize>(e: &Emit, name: &'static str, payload: &T) {
    e(
        name,
        serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
    );
}
