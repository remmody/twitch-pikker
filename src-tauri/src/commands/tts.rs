use super::St;
use crate::errors::{AppError, AppResult};
use crate::tts::download::DownloadProgress;
use crate::tts::models::{self, ModelValidation, TtsStatus};
use crate::tts::voices::{TtsEngine, VoiceInfo};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const MAX_TEST_CHARS: usize = 300;

#[tauri::command]
pub async fn list_tts_voices(state: St<'_>, engine: TtsEngine) -> AppResult<Vec<VoiceInfo>> {
    let dir = state.paths.models_dir(&state.settings());
    tokio::task::spawn_blocking(move || models::list_voices(engine, &dir))
        .await
        .map_err(|e| AppError::msg(e.to_string()))
}

/// Speak `text` (preview / "Test voice") with an explicit engine and voice. Runs offline.
#[tauri::command]
pub async fn test_tts(
    state: St<'_>,
    text: String,
    engine: TtsEngine,
    voice_id: String,
) -> AppResult<()> {
    let text: String = text.chars().take(MAX_TEST_CHARS).collect();
    if text.trim().is_empty() {
        return Err(AppError::msg("Enter some text to speak."));
    }
    state.pipeline.test_tts(text, engine, voice_id);
    Ok(())
}

/// Folder picker for the models folder; returns the chosen path (not yet applied).
#[tauri::command]
pub async fn pick_tts_model(app: AppHandle) -> AppResult<Option<String>> {
    let Some(dir) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = dir.into_path().map_err(|e| AppError::msg(e.to_string()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn validate_tts_model(path: String) -> AppResult<ModelValidation> {
    tokio::task::spawn_blocking(move || models::validate_folder(Path::new(&path)))
        .await
        .map_err(|e| AppError::msg(e.to_string()))
}

/// `Offline TTS: Ready / Model missing / Runtime missing` for the selected engine and voice.
#[tauri::command]
pub async fn get_tts_status(state: St<'_>) -> AppResult<TtsStatus> {
    let s = state.settings();
    let dir = state.paths.models_dir(&s);
    let runtime = match s.tts.engine {
        TtsEngine::Silero => s.tts.python.clone(),
        TtsEngine::Piper => s.tts.piper_binary.clone(),
    };
    Ok(models::status(s.tts.engine, &s.tts.voice, &dir, &runtime))
}

#[derive(Serialize)]
pub struct PackInfo {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    engine: TtsEngine,
    approx_mb: u32,
    default_selected: bool,
    installed: bool,
}

#[derive(Serialize)]
pub struct ModelPacks {
    models_dir: String,
    packs: Vec<PackInfo>,
}

#[tauri::command]
pub async fn get_model_packs(state: St<'_>) -> AppResult<ModelPacks> {
    let dir = state.paths.models_dir(&state.settings());
    Ok(ModelPacks {
        models_dir: dir.display().to_string(),
        packs: models::PACKS
            .iter()
            .map(|p| PackInfo {
                id: p.id,
                label: p.label,
                description: p.description,
                engine: p.engine,
                approx_mb: p.approx_mb,
                default_selected: p.default_selected,
                installed: models::pack_installed(p, &dir),
            })
            .collect(),
    })
}

fn on_done(state: &Arc<crate::state::AppState>) -> Arc<dyn Fn() + Send + Sync> {
    let state = state.clone();
    Arc::new(move || state.finish_install())
}

/// Starts downloading the selected packs. Only ever called from an explicit user click.
#[tauri::command]
pub async fn start_model_download(state: St<'_>, pack_ids: Vec<String>) -> AppResult<()> {
    let dir = state.paths.models_dir(&state.settings());
    state
        .downloader
        .start(&state.rt, pack_ids, dir, on_done(&state))
        .map_err(AppError::msg)
}

#[tauri::command]
pub async fn resume_model_download(state: St<'_>) -> AppResult<()> {
    state
        .downloader
        .resume(&state.rt, on_done(&state))
        .map_err(AppError::msg)
}

#[tauri::command]
pub async fn pause_model_download(state: St<'_>) -> AppResult<()> {
    state.downloader.pause();
    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(state: St<'_>) -> AppResult<()> {
    state.downloader.cancel();
    Ok(())
}

#[tauri::command]
pub async fn get_download_progress(state: St<'_>) -> AppResult<DownloadProgress> {
    Ok(state.downloader.progress())
}
