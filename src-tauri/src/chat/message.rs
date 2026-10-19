use chrono::{DateTime, Local};
use std::ops::Range;
use twitch_irc::message::PrivmsgMessage;

/// A chat line, reduced to what the app needs.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub time: DateTime<Local>,
    pub username: String,
    pub text: String,
    /// Emote character ranges within `text`.
    pub emotes: Vec<Range<usize>>,
    pub is_action: bool,
}

impl From<PrivmsgMessage> for ChatMessage {
    fn from(m: PrivmsgMessage) -> Self {
        Self {
            time: m.server_timestamp.with_timezone(&Local),
            username: m.sender.name,
            text: m.message_text,
            emotes: m.emotes.into_iter().map(|e| e.char_range).collect(),
            is_action: m.is_action,
        }
    }
}

impl ChatMessage {
    #[cfg(test)]
    pub fn log_line(&self) -> String {
        let star = if self.is_action { "* " } else { "" };
        format!(
            "[{}] {}: {star}{}",
            self.time.format("%H:%M:%S"),
            self.username,
            self.text
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::sanitizer::{CleanOpts, clean};
    use twitch_irc::message::IRCMessage;

    const RAW: &str = "@badge-info=;badges=;color=#FF0000;display-name=Tester;emotes=25:0-4;id=abc;mod=0;room-id=1;subscriber=0;tmi-sent-ts=1700000000000;turbo=0;user-id=2;user-type= :tester!tester@tester.tmi.twitch.tv PRIVMSG #chan :Kappa hello https://x.y";

    #[test]
    fn parses_privmsg_with_emotes() {
        let irc = IRCMessage::parse(RAW).unwrap();
        let msg: ChatMessage = PrivmsgMessage::try_from(irc).unwrap().into();
        assert_eq!(msg.username, "Tester");
        assert_eq!(msg.emotes, vec![0..5]);
        assert_eq!(
            clean(&msg.text, &msg.emotes, &CleanOpts::plain(100)),
            "hello link"
        );
        assert!(msg.log_line().ends_with("Tester: Kappa hello https://x.y"));
    }

    #[test]
    fn malformed_lines_are_errors_not_panics() {
        assert!(IRCMessage::parse("").is_err());
        assert!(PrivmsgMessage::try_from(IRCMessage::parse("PING :tmi").unwrap()).is_err());
    }
}
