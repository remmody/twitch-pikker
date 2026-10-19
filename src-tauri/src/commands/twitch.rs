use super::St;
use crate::config::normalize_channel;
use crate::errors::AppResult;
use crate::events::{EVT_CHAT_MESSAGE, emit};
use crate::twitch::{client, events::ConnStatus};
use serde_json::json;
use std::sync::Arc;

#[tauri::command]
pub async fn connect_to_channel(state: St<'_>, channel: String) -> AppResult<()> {
    let channel = normalize_channel(&channel)?;
    state.disconnect();
    let mut s = state.settings();
    s.twitch.channel = channel;
    state.commit(s.clone())?;

    let (pipeline, emitter) = (state.pipeline.clone(), state.emit.clone());
    let sink: client::MessageSink = Arc::new(move |msg| {
        pipeline.on_message(&msg);
        emit(
            &emitter,
            EVT_CHAT_MESSAGE,
            &json!({
                "time": msg.time.format("%H:%M:%S").to_string(),
                "username": msg.username,
                "text": msg.text,
                "is_action": msg.is_action,
            }),
        );
    });
    let token = client::spawn(&state.rt, s.twitch, sink, state.status_tx.clone());
    *state.session.lock().unwrap_or_else(|e| e.into_inner()) = Some(token);
    state.status_tx.send_replace(ConnStatus::Connecting);
    Ok(())
}

#[tauri::command]
pub async fn disconnect_from_channel(state: St<'_>) -> AppResult<()> {
    state.disconnect();
    Ok(())
}

#[tauri::command]
pub async fn get_connection_status(state: St<'_>) -> AppResult<ConnStatus> {
    Ok(state.status_tx.borrow().clone())
}
