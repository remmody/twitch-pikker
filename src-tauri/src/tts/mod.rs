//! Offline text-to-speech layer. To add an engine: implement [`TextToSpeech`] in a new
//! file, add a variant + voices in `voices.rs`, model discovery in `models.rs`, and a
//! match arm in `pipeline::Pipeline::engine`.

pub mod download;
pub mod models;
pub mod piper;
pub mod runtime;
pub mod silero;
pub mod voices;

use crate::errors::TtsError;
pub use voices::{SynthesizedAudio, VoiceConfig};

#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Turn `text` into a WAV using only local files. Must be cancel-safe.
    async fn synthesize(
        &self,
        text: &str,
        voice: &VoiceConfig,
    ) -> Result<SynthesizedAudio, TtsError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fake;

    #[async_trait::async_trait]
    impl TextToSpeech for Fake {
        async fn synthesize(
            &self,
            text: &str,
            _: &VoiceConfig,
        ) -> Result<SynthesizedAudio, TtsError> {
            if text.is_empty() {
                return Err(TtsError::Failed("empty".into()));
            }
            Ok(SynthesizedAudio {
                wav: text.as_bytes().to_vec(),
            })
        }
    }

    fn voice() -> VoiceConfig {
        VoiceConfig {
            voice: "x".into(),
            speed: 1.0,
            model_path: "m".into(),
            runtime: "r".into(),
        }
    }

    #[tokio::test]
    async fn trait_object_contract() {
        let tts: Arc<dyn TextToSpeech> = Arc::new(Fake);
        assert_eq!(tts.synthesize("hi", &voice()).await.unwrap().wav, b"hi");
        assert!(tts.synthesize("", &voice()).await.is_err());
    }

    #[tokio::test]
    async fn real_engines_report_missing_model_without_panicking() {
        let v = VoiceConfig {
            model_path: "definitely/missing.onnx".into(),
            ..voice()
        };
        for e in [
            &piper::PiperTts as &dyn TextToSpeech,
            &silero::SileroTts::new("script.py".into()),
        ] {
            assert!(matches!(
                e.synthesize("hi", &v).await,
                Err(TtsError::ModelMissing(_))
            ));
        }
    }

    #[test]
    fn every_engine_has_both_genders() {
        use voices::{Gender, TtsEngine};
        for k in TtsEngine::ALL {
            assert!(k.known_voices().iter().any(|v| v.gender == Gender::Female));
            assert!(k.known_voices().iter().any(|v| v.gender == Gender::Male));
        }
    }
}
