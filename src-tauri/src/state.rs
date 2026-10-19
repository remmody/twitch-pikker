//! Shared application state, managed by Tauri and handed to commands.

use crate::config::{self, AppPaths, AppSettings, SharedSettings};
use crate::errors::AppResult;
use crate::events::{EVT_MODELS_CHANGED, EVT_SETTINGS_UPDATED, Emit, emit};
use crate::pipeline::Pipeline;
use crate::tts::download::Downloader;
use crate::tts::models::{self, RuntimeState};
use crate::tts::runtime::{self, RuntimeInstaller};
use crate::tts::voices::TtsEngine;
use crate::twitch::events::ConnStatus;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub rt: tokio::runtime::Handle,
    pub settings: SharedSettings,
    pub paths: AppPaths,
    pub emit: Emit,
    pub pipeline: Arc<Pipeline>,
    pub downloader: Downloader,
    pub installer: RuntimeInstaller,
    /// Cached "can this Python import torch" result, keyed by interpreter path.
    pub torch_cache: Mutex<Option<(std::path::PathBuf, bool)>>,
    pub status_tx: watch::Sender<ConnStatus>,
    pub session: Mutex<Option<CancellationToken>>,
    /// One-shot message for the UI (e.g. "your config was invalid and was reset").
    pub startup_notice: Mutex<Option<String>>,
}

impl AppState {
    pub fn settings(&self) -> AppSettings {
        config::read(&self.settings)
    }

    /// Replace the settings, write them to disk and tell the UI.
    pub fn commit(&self, new: AppSettings) -> AppResult<()> {
        config::write(&self.settings, new.clone());
        let saved = new.save(&self.paths.config_file);
        emit(&self.emit, EVT_SETTINGS_UPDATED, &new);
        Ok(saved?)
    }

    /// Is the runtime for `engine` available? (Silero also checks that PyTorch imports.)
    pub async fn runtime_state(&self, engine: TtsEngine, s: &AppSettings) -> RuntimeState {
        let dir = self.paths.models_dir(s);
        match engine {
            TtsEngine::Piper => {
                if runtime::resolve_piper(&s.tts.piper_binary, &dir).is_some() {
                    RuntimeState::Ok
                } else {
                    RuntimeState::NoPiper
                }
            }
            TtsEngine::Silero => {
                let Some(py) = runtime::resolve_silero_python(&s.tts.python, &dir) else {
                    return RuntimeState::NoPython;
                };
                let cached = self
                    .torch_cache
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                    .filter(|(p, _)| *p == py);
                let ok = match cached {
                    Some((_, ok)) => ok,
                    None => {
                        let ok = runtime::check_torch(&py).await;
                        *self.torch_cache.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some((py, ok));
                        ok
                    }
                };
                if ok {
                    RuntimeState::Ok
                } else {
                    RuntimeState::NoTorch
                }
            }
        }
    }

    pub fn invalidate_runtime_cache(&self) {
        *self.torch_cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn disconnect(&self) {
        if let Some(t) = self
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            t.cancel();
        }
        self.status_tx.send_replace(ConnStatus::Disconnected);
    }

    /// Clean shutdown: leave Twitch, stop synthesis/playback, flush settings.
    pub fn shutdown(&self) {
        self.disconnect();
        self.downloader.pause();
        self.installer.cancel();
        self.pipeline.shutdown();
        if let Err(e) = self.settings().save(&self.paths.config_file) {
            tracing::error!("saving settings on exit: {e}");
        }
    }

    /// After a successful model download: refresh the UI, and if the selected voice isn't
    /// usable, switch to a sensible default (female first; Silero if its runtime exists).
    pub fn finish_install(&self) {
        let mut s = self.settings();
        let dir = self.paths.models_dir(&s);
        let usable = |e: TtsEngine, v: &str, s: &AppSettings| {
            let rt_ok = match e {
                TtsEngine::Silero => runtime::resolve_silero_python(&s.tts.python, &dir).is_some(),
                TtsEngine::Piper => runtime::resolve_piper(&s.tts.piper_binary, &dir).is_some(),
            };
            models::is_installed(e, v, &dir) && rt_ok
        };
        if !usable(s.tts.engine, &s.tts.voice, &s) {
            let candidates = [
                (TtsEngine::Silero, "kseniya"),
                (TtsEngine::Piper, "ru_RU-irina-medium"),
            ];
            let pick = candidates
                .iter()
                .find(|(e, v)| usable(*e, v, &s))
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|(e, v)| models::is_installed(*e, v, &dir))
                });
            if let Some((e, v)) = pick {
                s.tts.engine = *e;
                s.tts.voice = (*v).to_owned();
                if let Err(e) = self.commit(s) {
                    tracing::error!("saving default voice: {e}");
                }
            }
        }
        emit(&self.emit, EVT_MODELS_CHANGED, &json!({}));
    }
}
