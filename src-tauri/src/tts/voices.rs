use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    #[default]
    Silero,
    Piper,
}

impl TtsEngine {
    #[cfg(test)]
    pub const ALL: [TtsEngine; 2] = [TtsEngine::Silero, TtsEngine::Piper];

    pub fn label(self) -> &'static str {
        match self {
            Self::Silero => "Silero",
            Self::Piper => "Piper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    Female,
    Male,
    Unknown,
}

/// A voice an engine ships with. `id` is what goes into settings.
#[derive(Debug, Clone, Copy)]
pub struct KnownVoice {
    pub id: &'static str,
    pub name: &'static str,
    pub gender: Gender,
}

impl TtsEngine {
    /// Built-in Russian voices. Add new engines' voices here.
    pub fn known_voices(self) -> &'static [KnownVoice] {
        const SILERO: &[KnownVoice] = &[
            KnownVoice {
                id: "kseniya",
                name: "Kseniya",
                gender: Gender::Female,
            },
            KnownVoice {
                id: "xenia",
                name: "Xenia",
                gender: Gender::Female,
            },
            KnownVoice {
                id: "baya",
                name: "Baya",
                gender: Gender::Female,
            },
            KnownVoice {
                id: "aidar",
                name: "Aidar",
                gender: Gender::Male,
            },
            KnownVoice {
                id: "eugene",
                name: "Eugene",
                gender: Gender::Male,
            },
        ];
        const PIPER: &[KnownVoice] = &[
            KnownVoice {
                id: "ru_RU-irina-medium",
                name: "Irina",
                gender: Gender::Female,
            },
            KnownVoice {
                id: "ru_RU-denis-medium",
                name: "Denis",
                gender: Gender::Male,
            },
            KnownVoice {
                id: "ru_RU-dmitri-medium",
                name: "Dmitri",
                gender: Gender::Male,
            },
            KnownVoice {
                id: "ru_RU-ruslan-medium",
                name: "Ruslan",
                gender: Gender::Male,
            },
        ];
        match self {
            Self::Silero => SILERO,
            Self::Piper => PIPER,
        }
    }
}

/// Voice row sent to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceInfo {
    pub engine: TtsEngine,
    pub id: String,
    pub name: String,
    pub language: &'static str,
    pub gender: Gender,
    pub installed: bool,
}

/// Per-request settings handed to [`super::TextToSpeech::synthesize`].
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub voice: String,
    /// 1.0 = normal. Applied at playback, so engines may ignore it.
    pub speed: f32,
    /// Local model file for this voice (Silero `.pt` / Piper `.onnx`).
    pub model_path: PathBuf,
    /// Interpreter (Silero: python) or binary (Piper) to run.
    pub runtime: String,
}

/// Synthesized speech as an in-memory WAV file.
#[derive(Debug, Clone)]
pub struct SynthesizedAudio {
    pub wav: Vec<u8>,
}
