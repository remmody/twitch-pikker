//! `config.toml` model, persistence and validation. The same structs are sent to the
//! frontend as JSON (snake_case), so the TS types mirror these field names.

use crate::errors::ConfigError;
use crate::tts::voices::TtsEngine;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const SOUND_EXTENSIONS: [&str; 3] = ["wav", "mp3", "ogg"];
const APP_DIR: &str = "twitch-chat-reader";

/// Settings shared with background tasks. Never hold the guard across an `.await`.
pub type SharedSettings = Arc<RwLock<AppSettings>>;

/// Read-lock helper that survives poisoning (a panicked writer must not kill the app).
pub fn read(s: &SharedSettings) -> AppSettings {
    match s.read() {
        Ok(g) => g.clone(),
        Err(e) => e.into_inner().clone(),
    }
}

pub fn write(s: &SharedSettings, v: AppSettings) {
    match s.write() {
        Ok(mut g) => *g = v,
        Err(e) => *e.into_inner() = v,
    }
}

/// Where things live on disk. Per-user, platform-appropriate:
/// Linux `~/.local/share`, Windows `%LOCALAPPDATA%`, macOS `~/Library/Application Support`.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub default_models_dir: PathBuf,
    pub silero_script: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_DIR);
        Self {
            config_file: data_dir.join("config.toml"),
            default_models_dir: data_dir.join("models"),
            silero_script: data_dir.join("silero_server.py"),
            data_dir,
        }
    }

    pub fn models_dir(&self, s: &AppSettings) -> PathBuf {
        if s.tts.models_dir.trim().is_empty() {
            self.default_models_dir.clone()
        } else {
            PathBuf::from(s.tts.models_dir.trim())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub twitch: TwitchSettings,
    pub audio: AudioSettings,
    pub tts: TtsSettings,
    pub notifications: NotificationSettings,
    pub ui: UiSettings,
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TwitchSettings {
    pub channel: String,
    pub username: String,
    pub oauth_token: String,
}

// Manual Debug so a stray `{:?}` can never leak the token.
impl fmt::Debug for TwitchSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TwitchSettings")
            .field("channel", &self.channel)
            .field("username", &self.username)
            .field("oauth_token", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundChoice {
    #[default]
    DefaultPing,
    SoftPop,
    Telegram,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Empty = system default output device.
    pub output_device: String,
    pub sound: SoundChoice,
    /// Used when `sound == Custom`: a .wav/.mp3/.ogg path.
    pub custom_sound_path: String,
    pub notification_volume: u8,
    /// Minimum gap between notification sounds, 0..=5000 ms.
    pub cooldown_ms: u32,
    pub play_while_speaking: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            output_device: String::new(),
            sound: SoundChoice::DefaultPing,
            custom_sound_path: String::new(),
            notification_volume: 80,
            cooldown_ms: 300,
            play_while_speaking: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsSettings {
    pub enabled: bool,
    pub engine: TtsEngine,
    pub voice: String,
    pub speed: f32,
    pub volume: u8,
    pub max_message_chars: usize,
    pub queue_capacity: usize,
    pub announce_username: bool,
    pub username_template: String,
    pub read_emotes: bool,
    pub read_links: bool,
    /// Empty = the default per-user models folder.
    pub models_dir: String,
    pub python: String,
    pub piper_binary: String,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: TtsEngine::Silero,
            voice: "kseniya".into(),
            speed: 1.0,
            volume: 90,
            max_message_chars: 180,
            queue_capacity: 8,
            announce_username: true,
            username_template: "{username} says:".into(),
            read_emotes: false,
            read_links: false,
            models_dir: String::new(),
            python: if cfg!(windows) {
                "python".into()
            } else {
                "python3".into()
            },
            piper_binary: "piper".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    pub enabled: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: Theme,
}

/// Lowercases/trims and checks a Twitch login (1-25 of `[a-z0-9_]`, optional leading `#`).
pub fn normalize_channel(raw: &str) -> Result<String, ConfigError> {
    let name = raw.trim().trim_start_matches('#').to_ascii_lowercase();
    let ok = (1..=25).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(name)
    } else {
        Err(ConfigError::Channel(raw.to_owned()))
    }
}

impl AppSettings {
    /// Missing file → defaults. Malformed file → error (never overwritten silently).
    pub fn load(path: &Path) -> Result<AppSettings, ConfigError> {
        let p = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| ConfigError::Parse {
                path: p,
                message: e.to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
            Err(source) => Err(ConfigError::Io { path: p, source }),
        }
    }

    /// Write a sibling temp file, then rename over the target.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let p = path.display().to_string();
        let io = |source| ConfigError::Io {
            path: p.clone(),
            source,
        };
        let text = toml::to_string_pretty(self).map_err(|e| ConfigError::Parse {
            path: p.clone(),
            message: e.to_string(),
        })?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, path))
            .map_err(io)
    }

    /// Empty channel is fine (the user may type it in the UI).
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.twitch.channel.trim().is_empty() {
            normalize_channel(&self.twitch.channel)?;
        }
        if self.audio.sound == SoundChoice::Custom && !self.audio.custom_sound_path.is_empty() {
            validate_sound(&self.audio.custom_sound_path)?;
        }
        for (field, v) in [
            ("audio.notification_volume", self.audio.notification_volume),
            ("tts.volume", self.tts.volume),
        ] {
            if v > 100 {
                return Err(ConfigError::Range {
                    field,
                    min: 0.0,
                    max: 100.0,
                });
            }
        }
        if self.audio.cooldown_ms > 5000 {
            return Err(ConfigError::Range {
                field: "audio.cooldown_ms",
                min: 0.0,
                max: 5000.0,
            });
        }
        if !(0.5..=2.0).contains(&self.tts.speed) {
            return Err(ConfigError::Range {
                field: "tts.speed",
                min: 0.5,
                max: 2.0,
            });
        }
        if !(1..=1000).contains(&self.tts.max_message_chars) {
            return Err(ConfigError::Range {
                field: "tts.max_message_chars",
                min: 1.0,
                max: 1000.0,
            });
        }
        if !(1..=100).contains(&self.tts.queue_capacity) {
            return Err(ConfigError::Range {
                field: "tts.queue_capacity",
                min: 1.0,
                max: 100.0,
            });
        }
        Ok(())
    }
}

/// Cheap check (extension + existence). Decoding is verified when the sound is loaded.
pub fn validate_sound(path: &str) -> Result<(), ConfigError> {
    let err = |reason: &str| ConfigError::Sound {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !SOUND_EXTENSIONS.contains(&ext.as_str()) {
        return Err(err("unsupported format (use .wav, .mp3 or .ogg)"));
    }
    if !Path::new(path).is_file() {
        return Err(err("file not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_roundtrip() {
        let c = AppSettings::default();
        c.validate().unwrap();
        let back: AppSettings = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn json_roundtrip_matches_frontend_shape() {
        let v = serde_json::to_value(AppSettings::default()).unwrap();
        assert_eq!(v["tts"]["engine"], "silero");
        assert_eq!(v["audio"]["sound"], "default_ping");
        assert_eq!(v["ui"]["theme"], "dark");
        let back: AppSettings = serde_json::from_value(v).unwrap();
        assert_eq!(back, AppSettings::default());
    }

    #[test]
    fn partial_and_old_files_use_defaults() {
        let c: AppSettings =
            toml::from_str("[twitch]\nchannel = \"Foo\"\n[tts]\nenabled = true\nengine = \"piper\"\nunknown_old_key = 1").unwrap();
        assert!(c.tts.enabled && c.tts.engine == TtsEngine::Piper);
        assert_eq!(c.audio.notification_volume, 80);
    }

    #[test]
    fn channel_validation() {
        assert_eq!(normalize_channel(" #Some_Chan1 ").unwrap(), "some_chan1");
        assert!(normalize_channel("").is_err());
        assert!(normalize_channel("bad name").is_err());
        assert!(normalize_channel(&"a".repeat(26)).is_err());
    }

    #[test]
    fn rejects_bad_values() {
        let mut c = AppSettings::default();
        c.tts.volume = 101;
        assert!(c.validate().is_err());
        let mut c = AppSettings::default();
        c.tts.speed = 5.0;
        assert!(c.validate().is_err());
        let mut c = AppSettings::default();
        c.audio.cooldown_ms = 6000;
        assert!(c.validate().is_err());
        let mut c = AppSettings::default();
        c.audio.sound = SoundChoice::Custom;
        c.audio.custom_sound_path = "ping.flac".into();
        assert!(c.validate().is_err());
        c.audio.custom_sound_path = "missing.wav".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn save_and_load_roundtrip_on_disk() {
        let path = std::env::temp_dir().join(format!("tcr-test-{}.toml", std::process::id()));
        let mut c = AppSettings::default();
        c.twitch.channel = "abc".into();
        c.save(&path).unwrap();
        assert_eq!(AppSettings::load(&path).unwrap(), c);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn debug_hides_token() {
        let t = TwitchSettings {
            oauth_token: "secret123".into(),
            ..Default::default()
        };
        assert!(!format!("{t:?}").contains("secret123"));
    }
}
