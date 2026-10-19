//! Twitch Pikker — Tauri shell + Rust core. All chat, TTS and audio logic lives in Rust;
//! the web frontend only talks to it through commands and events.

mod audio;
mod chat;
mod commands;
mod config;
mod errors;
mod events;
mod pipeline;
mod state;
mod tts;
mod twitch;

use config::{AppPaths, AppSettings};
use events::{EVT_CHAT_STATUS, Emit, emit};
use state::AppState;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{Emitter, Manager, RunEvent};
use tracing_subscriber::EnvFilter;
use twitch::events::ConnStatus;

const SILERO_SCRIPT: &str = include_str!("../scripts/silero_server.py");

/// Load settings; a broken file is backed up and replaced by defaults (with a UI notice).
fn load_settings(paths: &AppPaths) -> (AppSettings, Option<String>) {
    let loaded = AppSettings::load(&paths.config_file).and_then(|s| s.validate().map(|()| s));
    match loaded {
        Ok(s) => (s, None),
        Err(e) => {
            let bak = paths.config_file.with_extension("toml.bad");
            let _ = std::fs::rename(&paths.config_file, &bak);
            tracing::warn!("{e}");
            (
                AppSettings::default(),
                Some(format!(
                    "{e}. Settings were reset; the old file is kept as {}.",
                    bak.display()
                )),
            )
        }
    }
}

fn install_silero_script(paths: &AppPaths) {
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&paths.data_dir)?;
        if std::fs::read_to_string(&paths.silero_script)
            .ok()
            .as_deref()
            != Some(SILERO_SCRIPT)
        {
            std::fs::write(&paths.silero_script, SILERO_SCRIPT)?;
        }
        Ok(())
    };
    if let Err(e) = write() {
        tracing::error!("cannot write Silero helper script: {e}");
    }
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = AppPaths::resolve();
            install_silero_script(&paths);
            let (settings, notice) = load_settings(&paths);
            let shared = Arc::new(RwLock::new(settings));

            let handle = app.handle().clone();
            let emitter: Emit = Arc::new(move |name, payload| {
                if let Err(e) = handle.emit(name, payload) {
                    tracing::debug!("emit {name}: {e}");
                }
            });

            let rt = tauri::async_runtime::handle().inner().clone();
            let pipeline =
                pipeline::Pipeline::new(&rt, shared.clone(), paths.clone(), emitter.clone());
            let (status_tx, mut status_rx) = tokio::sync::watch::channel(ConnStatus::Disconnected);

            // Push connection status changes to the UI (no polling).
            let e2 = emitter.clone();
            rt.spawn(async move {
                while status_rx.changed().await.is_ok() {
                    let s = status_rx.borrow_and_update().clone();
                    emit(&e2, EVT_CHAT_STATUS, &s);
                }
            });

            app.manage(Arc::new(AppState {
                rt,
                settings: shared,
                paths,
                downloader: tts::download::Downloader::new(emitter.clone()),
                emit: emitter,
                pipeline,
                status_tx,
                session: Mutex::new(None),
                startup_notice: Mutex::new(notice),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::twitch::connect_to_channel,
            commands::twitch::disconnect_from_channel,
            commands::twitch::get_connection_status,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_diagnostics,
            commands::settings::open_config_folder,
            commands::settings::open_models_folder,
            commands::settings::take_startup_notice,
            commands::settings::check_executable,
            commands::tts::list_tts_voices,
            commands::tts::test_tts,
            commands::tts::pick_tts_model,
            commands::tts::validate_tts_model,
            commands::tts::get_tts_status,
            commands::tts::get_model_packs,
            commands::tts::start_model_download,
            commands::tts::resume_model_download,
            commands::tts::pause_model_download,
            commands::tts::cancel_model_download,
            commands::tts::get_download_progress,
            commands::audio::test_notification_sound,
            commands::audio::pick_notification_sound,
            commands::audio::list_audio_devices,
            commands::audio::set_muted,
        ])
        .build(tauri::generate_context!());

    match app {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event
                && let Some(state) = handle.try_state::<Arc<AppState>>()
            {
                state.shutdown();
            }
        }),
        Err(e) => tracing::error!("failed to start the app: {e}"),
    }
}
