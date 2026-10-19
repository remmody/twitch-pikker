use super::St;
use crate::audio::{notification, player};
use crate::errors::{AppError, AppResult};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn test_notification_sound(state: St<'_>) -> AppResult<()> {
    state.pipeline.test_notification();
    Ok(())
}

/// Opens a file picker; returns the chosen path after checking that it decodes.
#[tauri::command]
pub async fn pick_notification_sound(app: AppHandle) -> AppResult<Option<String>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Sound (WAV, MP3, OGG)", &["wav", "mp3", "ogg"])
        .blocking_pick_file();
    let Some(file) = picked else { return Ok(None) };
    let path = file.into_path().map_err(|e| AppError::msg(e.to_string()))?;
    let path = path.to_string_lossy().into_owned();
    notification::load(&path)?;
    Ok(Some(path))
}

#[tauri::command]
pub async fn list_audio_devices() -> AppResult<Vec<String>> {
    tokio::task::spawn_blocking(player::list_devices)
        .await
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Temporarily silence chat-driven audio (not persisted; the UI remembers it).
#[tauri::command]
pub async fn set_muted(state: St<'_>, muted: bool) -> AppResult<()> {
    state.pipeline.set_muted(muted);
    Ok(())
}
