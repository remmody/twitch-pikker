//! Piper adapter: one local `piper` process per phrase, WAV via a temp file.
//! Nothing here touches the network.

use super::{SynthesizedAudio, TextToSpeech, VoiceConfig};
use crate::errors::TtsError;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct PiperTts;

#[async_trait::async_trait]
impl TextToSpeech for PiperTts {
    async fn synthesize(
        &self,
        text: &str,
        voice: &VoiceConfig,
    ) -> Result<SynthesizedAudio, TtsError> {
        if !voice.model_path.is_file() {
            return Err(TtsError::ModelMissing(voice.model_path.clone()));
        }
        let out = std::env::temp_dir().join(format!(
            "tcr-piper-{}-{}.wav",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut cmd = Command::new(&voice.runtime);
        cmd.arg("--model")
            .arg(&voice.model_path)
            .arg("--output_file")
            .arg(&out)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd
            .spawn()
            .map_err(|e| TtsError::Unavailable(format!("cannot start '{}': {e}", voice.runtime)))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.replace('\n', " ").as_bytes()).await?;
            stdin.write_all(b"\n").await?;
        }
        let result = tokio::time::timeout(Duration::from_secs(20), child.wait()).await;
        let wav = match result {
            Err(_) => Err(TtsError::Timeout),
            Ok(Err(e)) => Err(e.into()),
            Ok(Ok(st)) if !st.success() => Err(TtsError::Failed(format!("piper exited with {st}"))),
            Ok(Ok(_)) => tokio::fs::read(&out).await.map_err(TtsError::from),
        };
        let _ = tokio::fs::remove_file(&out).await;
        Ok(SynthesizedAudio { wav: wav? })
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod integration {
    use super::*;

    #[tokio::test]
    #[ignore = "needs piper on PATH and ru_RU-irina-medium.onnx in ./models"]
    async fn speaks_russian() {
        let v = VoiceConfig {
            voice: "ru_RU-irina-medium".into(),
            speed: 1.0,
            model_path: "models/ru_RU-irina-medium.onnx".into(),
            runtime: "piper".into(),
        };
        let a = PiperTts.synthesize("Привет", &v).await.unwrap();
        assert!(a.wav.starts_with(b"RIFF"));
    }
}
