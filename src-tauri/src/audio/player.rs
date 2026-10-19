//! Audio output lives on its own OS thread (output streams aren't `Send` everywhere).
//! Commands arrive over a bounded channel and are never awaited by callers.

use crate::audio::notification::builtin;
use crate::config::SoundChoice;
use crate::errors::AudioError;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, SyncSender, sync_channel};
use std::time::Duration;
use tokio::sync::oneshot;

/// What to play for a notification.
#[derive(Clone)]
pub enum Sound {
    Builtin(SoundChoice),
    /// Encoded WAV/MP3/OGG bytes of a custom file.
    Custom(Arc<[u8]>),
}

enum Cmd {
    Beep {
        sound: Sound,
        volume: f32,
    },
    Speech {
        wav: Vec<u8>,
        volume: f32,
        speed: f32,
        done: oneshot::Sender<()>,
    },
    StopSpeech,
}

#[derive(Clone)]
pub struct AudioHandle {
    tx: SyncSender<Cmd>,
}

impl AudioHandle {
    /// Fire-and-forget; dropped if the audio thread is saturated.
    pub fn beep(&self, sound: Sound, volume: f32) {
        let _ = self.tx.try_send(Cmd::Beep { sound, volume });
    }

    /// Queues speech after any speech already playing. The receiver resolves when
    /// playback finishes (or errors if it could not be played).
    pub fn speech(&self, wav: Vec<u8>, volume: f32, speed: f32) -> Option<oneshot::Receiver<()>> {
        let (done, rx) = oneshot::channel();
        self.tx
            .try_send(Cmd::Speech {
                wav,
                volume,
                speed,
                done,
            })
            .ok()?;
        Some(rx)
    }

    pub fn stop_speech(&self) {
        let _ = self.tx.try_send(Cmd::StopSpeech);
    }
}

/// Names of the available output devices.
pub fn list_devices() -> Vec<String> {
    let host = rodio::cpal::default_host();
    let Ok(devices) = host.output_devices() else {
        return vec![];
    };
    devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_owned()))
        .collect()
}

fn open_sink(device: &str) -> Result<rodio::MixerDeviceSink, AudioError> {
    fn err(e: impl std::fmt::Display) -> AudioError {
        AudioError::Device(e.to_string())
    }
    if device.is_empty() {
        return DeviceSinkBuilder::open_default_sink().map_err(err);
    }
    let host = rodio::cpal::default_host();
    let found = host
        .output_devices()
        .map_err(err)?
        .find(|d| d.description().is_ok_and(|desc| desc.name() == device));
    match found {
        Some(d) => DeviceSinkBuilder::from_device(d)
            .map_err(err)?
            .open_stream()
            .map_err(err),
        None => Err(AudioError::Device(format!(
            "output device '{device}' not found"
        ))),
    }
}

/// Opens `device` ("" = system default). The thread exits when all handles are dropped.
pub fn spawn(device: &str) -> Result<AudioHandle, AudioError> {
    let (tx, rx) = sync_channel::<Cmd>(32);
    let (init_tx, init_rx) = std::sync::mpsc::channel();
    let device = device.to_owned();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let sink = match open_sink(&device) {
                Ok(s) => s,
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };
            let _ = init_tx.send(Ok(()));
            let speech = Player::connect_new(sink.mixer());
            let mut pending: Option<oneshot::Sender<()>> = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(Cmd::Beep { sound, volume }) => {
                        let p = Player::connect_new(sink.mixer());
                        p.set_volume(volume);
                        match sound {
                            Sound::Builtin(c) => p.append(builtin(c)),
                            Sound::Custom(bytes) => match Decoder::new(Cursor::new(bytes)) {
                                Ok(src) => p.append(src),
                                Err(e) => {
                                    tracing::warn!("notification sound failed to decode: {e}");
                                    p.append(builtin(SoundChoice::DefaultPing));
                                }
                            },
                        }
                        p.detach();
                    }
                    Ok(Cmd::Speech {
                        wav,
                        volume,
                        speed,
                        done,
                    }) => match Decoder::new(Cursor::new(wav)) {
                        Ok(src) => {
                            speech.set_volume(volume);
                            speech.append(src.speed(speed));
                            pending = Some(done);
                        }
                        Err(e) => tracing::warn!("cannot decode synthesized speech: {e}"), // `done` dropped
                    },
                    Ok(Cmd::StopSpeech) => {
                        speech.clear();
                        speech.play(); // clear() may leave the player paused
                        pending = None;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if pending.is_some()
                    && speech.empty()
                    && let Some(done) = pending.take()
                {
                    let _ = done.send(());
                }
            }
        })
        .map_err(|e| AudioError::Device(e.to_string()))?;
    init_rx
        .recv()
        .map_err(|_| AudioError::Device("audio thread died during startup".into()))??;
    Ok(AudioHandle { tx })
}

#[cfg(all(test, feature = "integration-tests"))]
mod integration {
    #[test]
    #[ignore = "needs an audio output device"]
    fn opens_default_device() {
        super::spawn("")
            .unwrap()
            .beep(super::Sound::Builtin(super::SoundChoice::DefaultPing), 0.1);
    }
}
