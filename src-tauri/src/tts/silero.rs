//! Silero adapter. The v4_ru model is a TorchScript/torch.package file, so it runs in a
//! local Python helper (`silero_server.py`) that loads the model *from disk* — no
//! `torch.hub`, no downloads. One helper process stays alive between phrases.
//!   request:  `<speaker>\t<text>\n`
//!   response: `OK <wav path>\n` or `ERR <message>\n` (after an initial `READY\n`).

use super::{SynthesizedAudio, TextToSpeech, VoiceConfig};
use crate::errors::TtsError;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

struct Helper {
    _child: Child, // kill_on_drop
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// (python, model) the helper was started with; a change means restart.
    key: (String, PathBuf),
    /// Last line the helper wrote to stderr (e.g. a Python import error).
    stderr_tail: Arc<std::sync::Mutex<String>>,
}

pub struct SileroTts {
    script: PathBuf,
    helper: Mutex<Option<Helper>>,
}

impl SileroTts {
    pub fn new(script: PathBuf) -> Self {
        Self {
            script,
            helper: Mutex::new(None),
        }
    }

    async fn start(&self, voice: &VoiceConfig) -> Result<Helper, TtsError> {
        if !self.script.is_file() {
            return Err(TtsError::Unavailable(format!(
                "Silero helper script missing: {}",
                self.script.display()
            )));
        }
        let mut cmd = Command::new(&voice.runtime);
        cmd.arg(&self.script)
            .arg(&voice.model_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd
            .spawn()
            .map_err(|e| TtsError::Unavailable(format!("cannot start '{}': {e}", voice.runtime)))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(TtsError::Unavailable("helper pipes unavailable".into()));
        };
        let stderr_tail = Arc::new(std::sync::Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail = stderr_tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if !l.trim().is_empty() {
                        *tail.lock().unwrap_or_else(|e| e.into_inner()) = l.trim().to_owned();
                    }
                }
            });
        }
        let mut helper = Helper {
            _child: child,
            stdin,
            stdout: BufReader::new(stdout),
            key: (voice.runtime.clone(), voice.model_path.clone()),
            stderr_tail,
        };
        let line = match read_line(&mut helper.stdout, STARTUP_TIMEOUT).await {
            Ok(l) => l,
            Err(e) => return Err(explain(e, &helper.stderr_tail).await),
        };
        if line.trim() != "READY" {
            return Err(TtsError::Unavailable(format!(
                "Silero helper failed to start (is PyTorch installed?): {}",
                line.trim()
            )));
        }
        Ok(helper)
    }

    async fn request(helper: &mut Helper, speaker: &str, text: &str) -> Result<Vec<u8>, TtsError> {
        let text: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        helper
            .stdin
            .write_all(format!("{speaker}\t{text}\n").as_bytes())
            .await?;
        helper.stdin.flush().await?;
        let line = match read_line(&mut helper.stdout, REQUEST_TIMEOUT).await {
            Ok(l) => l,
            Err(e) => return Err(explain(e, &helper.stderr_tail).await),
        };
        match line.trim().split_once(' ') {
            Some(("OK", path)) => {
                let wav = tokio::fs::read(path).await;
                let _ = tokio::fs::remove_file(path).await;
                Ok(wav?)
            }
            Some(("ERR", msg)) => Err(TtsError::Failed(msg.to_owned())),
            _ => Err(TtsError::Unavailable(format!(
                "bad helper reply: {}",
                line.trim()
            ))),
        }
    }
}

/// Adds the helper's last stderr line to a "helper exited" error.
async fn explain(e: TtsError, tail: &std::sync::Mutex<String>) -> TtsError {
    match e {
        TtsError::Unavailable(m) if m == "Silero helper exited" => {
            tokio::time::sleep(Duration::from_millis(200)).await; // let the stderr reader catch up
            let t = tail.lock().unwrap_or_else(|e| e.into_inner()).clone();
            TtsError::Unavailable(if t.is_empty() { m } else { format!("{m}: {t}") })
        }
        other => other,
    }
}

async fn read_line(r: &mut BufReader<ChildStdout>, timeout: Duration) -> Result<String, TtsError> {
    let mut line = String::new();
    match tokio::time::timeout(timeout, r.read_line(&mut line)).await {
        Err(_) => Err(TtsError::Timeout),
        Ok(Err(e)) => Err(e.into()),
        Ok(Ok(0)) => Err(TtsError::Unavailable("Silero helper exited".into())),
        Ok(Ok(_)) => Ok(line),
    }
}

#[async_trait::async_trait]
impl TextToSpeech for SileroTts {
    async fn synthesize(
        &self,
        text: &str,
        voice: &VoiceConfig,
    ) -> Result<SynthesizedAudio, TtsError> {
        if !voice.model_path.is_file() {
            return Err(TtsError::ModelMissing(voice.model_path.clone()));
        }
        let mut guard = self.helper.lock().await;
        let key = (voice.runtime.clone(), voice.model_path.clone());
        if guard.as_ref().is_some_and(|h| h.key != key) {
            *guard = None;
        }
        if guard.is_none() {
            *guard = Some(self.start(voice).await?);
        }
        let Some(helper) = guard.as_mut() else {
            return Err(TtsError::Unavailable("helper not running".into()));
        };
        let res = Self::request(helper, &voice.voice, text).await;
        // Everything except a per-phrase `ERR` leaves the pipe in an unknown state.
        if !matches!(res, Ok(_) | Err(TtsError::Failed(_))) {
            *guard = None;
        }
        Ok(SynthesizedAudio { wav: res? })
    }
}
