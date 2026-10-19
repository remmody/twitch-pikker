//! Tauri commands: thin wrappers that validate input and call into the core modules.

pub mod audio;
pub mod settings;
pub mod tts;
pub mod twitch;

use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

pub type St<'a> = State<'a, Arc<AppState>>;
