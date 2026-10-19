use super::St;
use crate::config::{AppSettings, normalize_channel};
use crate::errors::{AppError, AppResult};
use crate::tts::models;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn get_settings(state: St<'_>) -> AppResult<AppSettings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn save_settings(state: St<'_>, settings: AppSettings) -> AppResult<()> {
    let mut new = settings;
    if !new.twitch.channel.trim().is_empty() {
        new.twitch.channel = normalize_channel(&new.twitch.channel)?;
    }
    new.validate()?;
    let old = state.settings();
    let device_changed = old.audio.output_device != new.audio.output_device;
    let device = new.audio.output_device.clone();
    state.commit(new)?;
    if device_changed {
        let pipeline = state.pipeline.clone();
        tokio::task::spawn_blocking(move || pipeline.set_audio_device(&device))
            .await
            .map_err(|e| AppError::msg(e.to_string()))?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Diagnostics {
    connection: String,
    engine: String,
    voice: String,
    audio_backend: String,
    queue_len: usize,
    last_error: Option<String>,
    config_path: String,
    models_dir: String,
    version: &'static str,
}

#[tauri::command]
pub async fn get_diagnostics(state: St<'_>) -> AppResult<Diagnostics> {
    let s = state.settings();
    Ok(Diagnostics {
        connection: state.status_tx.borrow().label(),
        engine: s.tts.engine.label().to_owned(),
        voice: s.tts.voice.clone(),
        audio_backend: if state.pipeline.audio_ok() {
            "OK".into()
        } else {
            "Unavailable".into()
        },
        queue_len: state.pipeline.queue_len(),
        last_error: state.pipeline.last_error(),
        config_path: state.paths.config_file.display().to_string(),
        models_dir: state.paths.models_dir(&s).display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
pub async fn open_config_folder(app: AppHandle, state: St<'_>) -> AppResult<()> {
    let dir = state.paths.data_dir.clone();
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::msg(e.to_string()))
}

#[tauri::command]
pub async fn open_models_folder(app: AppHandle, state: St<'_>) -> AppResult<()> {
    let dir = state.paths.models_dir(&state.settings());
    std::fs::create_dir_all(&dir).map_err(|e| AppError::msg(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Taken once; lets the UI show e.g. "config was invalid and has been reset".
#[tauri::command]
pub async fn take_startup_notice(state: St<'_>) -> AppResult<Option<String>> {
    Ok(state
        .startup_notice
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take())
}

/// Used by the settings screen to check an arbitrary runtime path.
#[tauri::command]
pub async fn check_executable(name: String) -> AppResult<bool> {
    Ok(models::find_executable(&name).is_some())
}
