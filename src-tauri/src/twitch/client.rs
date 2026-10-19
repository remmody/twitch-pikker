//! Twitch chat listener. `twitch-irc` already handles PING/PONG, send rate limits and
//! per-connection reconnects; this layer adds session-level supervision: if the
//! channel join isn't confirmed for too long, the whole client is rebuilt with
//! exponential backoff + jitter.

use super::events::ConnStatus;
use crate::chat::message::ChatMessage;
use crate::config::{TwitchSettings, normalize_channel};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::message::ServerMessage;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient};

/// Called for every chat line on the listener task. **Must not block.**
pub type MessageSink = Arc<dyn Fn(ChatMessage) + Send + Sync>;

const JOIN_TIMEOUT: Duration = Duration::from_secs(30);

enum SessionEnd {
    Cancelled,
    Lost {
        was_connected: bool,
        reason: &'static str,
    },
    Fatal(String),
}

/// Starts the listener; cancel the returned token to disconnect. The task does not
/// publish a final status on cancellation, so the caller owns that (avoids races on
/// quick reconnects).
pub fn spawn(
    rt: &tokio::runtime::Handle,
    cfg: TwitchSettings,
    sink: MessageSink,
    status: watch::Sender<ConnStatus>,
) -> CancellationToken {
    let cancel = CancellationToken::new();
    rt.spawn(run(cfg, sink, status, cancel.clone()));
    cancel
}

/// `base * 2^attempt` capped at 60 s, scaled by 0.75..1.25 (`jitter` in 0..1).
pub fn backoff(attempt: u32, jitter: f64) -> Duration {
    let secs = (2f64.powi(attempt.min(6) as i32)).min(60.0);
    Duration::from_secs_f64(secs * (0.75 + 0.5 * jitter.clamp(0.0, 1.0)))
}

fn jitter() -> f64 {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(n % 1000) / 1000.0
}

async fn run(
    cfg: TwitchSettings,
    sink: MessageSink,
    status: watch::Sender<ConnStatus>,
    cancel: CancellationToken,
) {
    let channel = match normalize_channel(&cfg.channel) {
        Ok(c) => c,
        Err(e) => {
            status.send_replace(ConnStatus::Error(e.to_string()));
            return;
        }
    };
    let mut attempt = 0u32;
    loop {
        status.send_replace(if attempt == 0 {
            ConnStatus::Connecting
        } else {
            ConnStatus::Reconnecting
        });
        match session(&cfg, &channel, &sink, &status, &cancel).await {
            SessionEnd::Cancelled => return,
            SessionEnd::Fatal(e) => {
                status.send_replace(ConnStatus::Error(e));
                return;
            }
            SessionEnd::Lost {
                was_connected,
                reason,
            } => {
                attempt = if was_connected { 1 } else { attempt + 1 };
                let wait = backoff(attempt - 1, jitter());
                tracing::warn!("twitch session lost ({reason}); retrying in {wait:.1?}");
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(wait) => {}
                }
            }
        }
    }
}

async fn session(
    cfg: &TwitchSettings,
    channel: &str,
    sink: &MessageSink,
    status: &watch::Sender<ConnStatus>,
    cancel: &CancellationToken,
) -> SessionEnd {
    let token = cfg.oauth_token.trim().trim_start_matches("oauth:");
    let login = if token.is_empty() || cfg.username.trim().is_empty() {
        StaticLoginCredentials::anonymous()
    } else {
        StaticLoginCredentials::new(
            cfg.username.trim().to_ascii_lowercase(),
            Some(token.to_owned()),
        )
    };
    let (mut incoming, client) = TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(
        ClientConfig::new_simple(login),
    );
    if let Err(e) = client.join(channel.to_owned()) {
        return SessionEnd::Fatal(e.to_string());
    }

    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let (mut connected, mut since) = (false, Instant::now());
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                client.part(channel.to_owned());
                return SessionEnd::Cancelled;
            }
            msg = incoming.recv() => match msg {
                Some(ServerMessage::Privmsg(m)) => sink(m.into()),
                Some(_) => {}
                None => return SessionEnd::Lost { was_connected: connected, reason: "stream closed" },
            },
            _ = tick.tick() => {
                let (_, confirmed) = client.get_channel_status(channel.to_owned()).await;
                if confirmed {
                    (connected, since) = (true, Instant::now());
                    status.send_replace(ConnStatus::Connected);
                } else {
                    status.send_replace(if connected { ConnStatus::Reconnecting } else { ConnStatus::Connecting });
                    if since.elapsed() > JOIN_TIMEOUT {
                        return SessionEnd::Lost { was_connected: connected, reason: "join not confirmed" };
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(0, 0.5), Duration::from_secs(1));
        assert_eq!(backoff(3, 0.5), Duration::from_secs(8));
        assert!(backoff(20, 1.0) <= Duration::from_secs(75));
        assert!(backoff(2, 0.0) < backoff(2, 1.0));
    }
}
