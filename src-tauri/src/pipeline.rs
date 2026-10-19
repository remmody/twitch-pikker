//! Chat message → beep / speech. `on_message` is called from the Twitch task and never
//! blocks: beeps go to the audio thread via `try_send`, speech goes to a bounded
//! drop-oldest queue drained by a single worker task (sequential, never overlapping).

use crate::audio::{
    notification,
    player::{self, AudioHandle, Sound},
};
use crate::chat::{
    message::ChatMessage,
    sanitizer::{self, CleanOpts},
};
use crate::config::{AppPaths, AudioSettings, SharedSettings, SoundChoice, read};
use crate::events::{EVT_AUDIO_ERROR, EVT_QUEUE_UPDATED, EVT_TTS_ERROR, Emit, emit};
use crate::tts::TextToSpeech;
use crate::tts::models;
use crate::tts::piper::PiperTts;
use crate::tts::silero::SileroTts;
use crate::tts::voices::{TtsEngine, VoiceConfig};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;

/// FIFO that evicts the oldest item when full.
pub struct DropOldestQueue<T> {
    items: Mutex<VecDeque<T>>,
    notify: Notify,
}

impl<T> DropOldestQueue<T> {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    /// Returns how many stale items were dropped to make room.
    pub fn push(&self, item: T, capacity: usize) -> usize {
        let mut dropped = 0;
        {
            let mut q = lock(&self.items);
            while q.len() >= capacity.max(1) {
                q.pop_front();
                dropped += 1;
            }
            q.push_back(item);
        }
        self.notify.notify_one();
        dropped
    }

    pub async fn pop(&self) -> T {
        loop {
            if let Some(item) = lock(&self.items).pop_front() {
                return item;
            }
            self.notify.notified().await;
        }
    }

    pub fn len(&self) -> usize {
        lock(&self.items).len()
    }

    pub fn clear(&self) {
        lock(&self.items).clear();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

type CustomSoundCache = Option<(String, Option<Arc<[u8]>>)>;

struct Job {
    text: String,
    /// Test/preview: speak even if TTS (or mute) is switched off, with this engine/voice.
    over: Option<(TtsEngine, String)>,
}

pub struct Pipeline {
    settings: SharedSettings,
    paths: AppPaths,
    emit: Emit,
    audio: RwLock<Option<AudioHandle>>,
    queue: DropOldestQueue<Job>,
    silero: SileroTts,
    piper: PiperTts,
    /// (path, decoded bytes) of the custom sound currently cached.
    sound: Mutex<CustomSoundCache>,
    tts_error: Mutex<Option<String>>,
    audio_error: Mutex<Option<String>>,
    muted: AtomicBool,
    speaking: AtomicBool,
    last_beep: Mutex<Option<Instant>>,
    cancel: CancellationToken,
}

impl Pipeline {
    pub fn new(
        rt: &tokio::runtime::Handle,
        settings: SharedSettings,
        paths: AppPaths,
        emit: Emit,
    ) -> Arc<Self> {
        let this = Arc::new(Self {
            silero: SileroTts::new(paths.silero_script.clone()),
            piper: PiperTts,
            settings,
            paths,
            emit,
            audio: RwLock::new(None),
            queue: DropOldestQueue::new(),
            sound: Mutex::new(None),
            tts_error: Mutex::new(None),
            audio_error: Mutex::new(None),
            muted: AtomicBool::new(false),
            speaking: AtomicBool::new(false),
            last_beep: Mutex::new(None),
            cancel: CancellationToken::new(),
        });
        let device = read(&this.settings).audio.output_device;
        this.set_audio_device(&device);
        rt.spawn(this.clone().worker());
        this
    }

    /// (Re)open the audio output. Falls back to the system default if `device` is unavailable.
    pub fn set_audio_device(&self, device: &str) {
        let opened = player::spawn(device).or_else(|e| {
            if device.is_empty() {
                Err(e)
            } else {
                player::spawn("").map_err(|_| e)
            }
        });
        match opened {
            Ok(h) => {
                *self.audio.write().unwrap_or_else(|e| e.into_inner()) = Some(h);
                let note = (!device.is_empty()
                    && player::list_devices().iter().all(|d| d != device))
                .then(|| format!("Output device '{device}' not found; using the system default."));
                self.report_audio(note);
            }
            Err(e) => {
                tracing::error!("{e}");
                *self.audio.write().unwrap_or_else(|e| e.into_inner()) = None;
                self.report_audio(Some(e.to_string()));
            }
        }
    }

    fn audio(&self) -> Option<AudioHandle> {
        self.audio.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn audio_ok(&self) -> bool {
        self.audio().is_some()
    }

    fn report_tts(&self, msg: Option<String>) {
        *lock(&self.tts_error) = msg.clone();
        emit(&self.emit, EVT_TTS_ERROR, &json!({ "message": msg }));
    }

    fn report_audio(&self, msg: Option<String>) {
        *lock(&self.audio_error) = msg.clone();
        emit(&self.emit, EVT_AUDIO_ERROR, &json!({ "message": msg }));
    }

    fn emit_queue(&self) {
        emit(
            &self.emit,
            EVT_QUEUE_UPDATED,
            &json!({ "len": self.queue.len() }),
        );
    }

    /// Latest user-visible problem (TTS engine, audio device, sound file), if any.
    pub fn last_error(&self) -> Option<String> {
        lock(&self.tts_error)
            .clone()
            .or_else(|| lock(&self.audio_error).clone())
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
        if let Some(a) = self.audio() {
            a.stop_speech();
        }
    }

    /// Temporarily silences everything chat-driven without touching saved settings.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
        if muted {
            self.queue.clear();
            self.emit_queue();
            if let Some(a) = self.audio() {
                a.stop_speech();
            }
        }
    }

    /// Non-blocking hot path.
    pub fn on_message(&self, msg: &ChatMessage) {
        if self.muted.load(Ordering::Relaxed) {
            return;
        }
        let s = read(&self.settings);
        if s.notifications.enabled {
            self.notify(&s.audio, false);
        }
        if s.tts.enabled {
            let t = &s.tts;
            let opts = CleanOpts {
                max_chars: t.max_message_chars,
                read_emotes: t.read_emotes,
                read_links: t.read_links,
            };
            let body = sanitizer::clean(&msg.text, &msg.emotes, &opts);
            if body.is_empty() {
                return;
            }
            let text = if t.announce_username {
                let name = sanitizer::clean(&msg.username, &[], &CleanOpts::plain(40));
                sanitizer::announce(&t.username_template, &name, &body)
            } else {
                body
            };
            let dropped = self.queue.push(Job { text, over: None }, t.queue_capacity);
            if dropped > 0 {
                tracing::debug!("speech queue full, dropped {dropped} oldest");
            }
            self.emit_queue();
        }
    }

    /// Speak `text` with an explicit engine/voice (voice preview / Test voice).
    pub fn test_tts(&self, text: String, engine: TtsEngine, voice: String) {
        self.queue.push(
            Job {
                text,
                over: Some((engine, voice)),
            },
            8,
        );
        self.emit_queue();
    }

    pub fn test_notification(&self) {
        let a = read(&self.settings).audio;
        self.notify(&a, true);
    }

    fn notify(&self, a: &AudioSettings, force: bool) {
        let Some(audio) = self.audio() else { return };
        if !force {
            if !a.play_while_speaking && self.speaking.load(Ordering::Relaxed) {
                return;
            }
            let mut last = lock(&self.last_beep);
            let gap = Duration::from_millis(u64::from(a.cooldown_ms));
            if last.is_some_and(|t| t.elapsed() < gap) {
                return;
            }
            *last = Some(Instant::now());
        }
        audio.beep(self.sound(a), f32::from(a.notification_volume) / 100.0);
    }

    fn sound(&self, a: &AudioSettings) -> Sound {
        if a.sound != SoundChoice::Custom {
            return Sound::Builtin(a.sound);
        }
        let mut cache = lock(&self.sound);
        if let Some((p, bytes)) = cache.as_ref()
            && *p == a.custom_sound_path
        {
            return bytes
                .clone()
                .map_or(Sound::Builtin(SoundChoice::DefaultPing), Sound::Custom);
        }
        let bytes = match notification::load(&a.custom_sound_path) {
            Ok(b) => {
                self.report_audio(None);
                Some(b)
            }
            Err(e) => {
                tracing::warn!("{e}");
                self.report_audio(Some(e.to_string()));
                None // falls back to the built-in ping
            }
        };
        *cache = Some((a.custom_sound_path.clone(), bytes.clone()));
        bytes.map_or(Sound::Builtin(SoundChoice::DefaultPing), Sound::Custom)
    }

    fn engine(&self, kind: TtsEngine) -> &dyn TextToSpeech {
        match kind {
            TtsEngine::Silero => &self.silero,
            TtsEngine::Piper => &self.piper,
        }
    }

    async fn worker(self: Arc<Self>) {
        let mut playing: Option<oneshot::Receiver<()>> = None;
        loop {
            let job = tokio::select! {
                _ = self.cancel.cancelled() => return,
                _ = async { if let Some(rx) = playing.as_mut() { let _ = rx.await; } }, if playing.is_some() => {
                    playing = None;
                    self.speaking.store(false, Ordering::Relaxed);
                    continue;
                }
                j = self.queue.pop() => j,
            };
            self.emit_queue();

            let s = read(&self.settings);
            let forced = job.over.is_some();
            if (!s.tts.enabled && !forced) || (self.muted.load(Ordering::Relaxed) && !forced) {
                continue;
            }
            let Some(audio) = self.audio() else { continue };
            let (kind, voice_id) = job
                .over
                .clone()
                .unwrap_or((s.tts.engine, s.tts.voice.clone()));
            let dir = self.paths.models_dir(&s);
            let voice = VoiceConfig {
                model_path: models::model_path(kind, &voice_id, &dir),
                runtime: match kind {
                    TtsEngine::Silero => s.tts.python.clone(),
                    TtsEngine::Piper => s.tts.piper_binary.clone(),
                },
                voice: voice_id,
                speed: s.tts.speed,
            };
            let result = tokio::select! {
                _ = self.cancel.cancelled() => return,
                r = self.engine(kind).synthesize(&job.text, &voice) => r,
            };
            match result {
                Ok(a) => {
                    self.report_tts(None);
                    // Synthesis of this phrase overlapped with playback of the previous one.
                    if let Some(rx) = playing.take() {
                        tokio::select! {
                            _ = self.cancel.cancelled() => return,
                            _ = rx => {}
                        }
                    }
                    playing = audio.speech(a.wav, f32::from(s.tts.volume) / 100.0, voice.speed);
                    self.speaking.store(playing.is_some(), Ordering::Relaxed);
                }
                Err(e) => {
                    tracing::warn!("TTS ({}) failed: {e}", kind.label());
                    self.report_tts(Some(format!("{}: {e}", kind.label())));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_drops_oldest() {
        let q = DropOldestQueue::new();
        assert_eq!(q.push(1, 3), 0);
        q.push(2, 3);
        q.push(3, 3);
        assert_eq!(q.push(4, 3), 1);
        assert_eq!(q.len(), 3);
        assert_eq!(
            lock(&q.items).iter().copied().collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
    }

    #[test]
    fn shrinking_capacity_evicts_extra() {
        let q = DropOldestQueue::new();
        for i in 0..5 {
            q.push(i, 10);
        }
        assert_eq!(q.push(5, 2), 4);
        assert_eq!(q.len(), 2);
        q.clear();
        assert_eq!(q.len(), 0);
    }

    #[tokio::test]
    async fn pop_waits_for_push() {
        let q = Arc::new(DropOldestQueue::new());
        let q2 = q.clone();
        let h = tokio::spawn(async move { q2.pop().await });
        tokio::task::yield_now().await;
        q.push(7, 2);
        assert_eq!(h.await.unwrap(), 7);
    }
}
