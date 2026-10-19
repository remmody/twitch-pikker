use super::St;
use crate::errors::{AppError, AppResult};
use crate::tts::download::DownloadProgress;
use crate::tts::models::{self, ModelValidation, RuntimeState, TtsStatus};
use crate::tts::runtime::{self, RuntimeProgress};
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
    let rt = state.runtime_state(s.tts.engine, &s).await;
    Ok(models::status(s.tts.engine, &s.tts.voice, &dir, rt))
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
    let s = state.settings();
    let dir = state.paths.models_dir(&s);
    Ok(ModelPacks {
        models_dir: dir.display().to_string(),
        packs: models::PACKS
            .iter()
            .filter(|p| !p.files.is_empty()) // no Piper build for this platform => no pack
            .map(|p| PackInfo {
                id: p.id,
                label: p.label,
                description: p.description,
                engine: p.engine,
                approx_mb: p.approx_mb,
                default_selected: p.default_selected,
                installed: if p.id == "piper_runtime" {
                    runtime::resolve_piper(&s.tts.piper_binary, &dir).is_some()
                } else {
                    models::pack_installed(p, &dir)
                },
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

#[derive(Serialize)]
pub struct RuntimeInfo {
    piper: bool,
    python: bool,
    torch: bool,
    /// Python can be installed automatically (Windows, via winget).
    can_install_python: bool,
}

/// What the offline engines need and what is present, for the setup wizard.
#[tauri::command]
pub async fn get_runtime_info(state: St<'_>) -> AppResult<RuntimeInfo> {
    let s = state.settings();
    let dir = state.paths.models_dir(&s);
    Ok(RuntimeInfo {
        piper: runtime::resolve_piper(&s.tts.piper_binary, &dir).is_some(),
        python: runtime::resolve_silero_python(&s.tts.python, &dir).is_some(),
        torch: state.runtime_state(TtsEngine::Silero, &s).await == RuntimeState::Ok,
        can_install_python: cfg!(windows),
    })
}

/// Installs Python (Windows) and/or PyTorch (private venv). Only called from an explicit user click.
#[tauri::command]
pub async fn install_runtimes(state: St<'_>, python: bool, pytorch: bool) -> AppResult<()> {
    let s = state.settings();
    let dir = state.paths.models_dir(&s);
    let st = state.inner().clone();
    state.invalidate_runtime_cache();
    let done: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        st.invalidate_runtime_cache();
        st.finish_install();
    });
    state
        .installer
        .start(&state.rt, python, pytorch, s.tts.python, dir, done)
        .map_err(AppError::msg)
}

#[tauri::command]
pub async fn cancel_runtime_install(state: St<'_>) -> AppResult<()> {
    state.installer.cancel();
    Ok(())
}

#[tauri::command]
pub async fn get_runtime_progress(state: St<'_>) -> AppResult<RuntimeProgress> {
    Ok(state.installer.progress())
}

/// Forget the cached PyTorch check (after the user installed something by hand).
#[tauri::command]
pub async fn recheck_runtimes(state: St<'_>) -> AppResult<()> {
    state.invalidate_runtime_cache();
    state.finish_install();
    Ok(())
}
