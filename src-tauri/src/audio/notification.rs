use crate::config::{SoundChoice, validate_sound};
use crate::errors::AudioError;
use rodio::Decoder;
use rodio::buffer::SamplesBuffer;
use std::io::Cursor;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

const MAX_SOUND_BYTES: u64 = 8 * 1024 * 1024;
const SAMPLE_RATE: u32 = 44_100;

/// `(frequency Hz, duration ms, decay)` notes, played back to back.
fn notes(choice: SoundChoice) -> &'static [(f32, u32, f32)] {
    match choice {
        SoundChoice::SoftPop => &[(520.0, 90, 28.0)],
        SoundChoice::Telegram => &[(1318.5, 70, 20.0), (1760.0, 130, 16.0)],
        _ => &[(880.0, 140, 14.0)],
    }
}

fn synth(choice: SoundChoice) -> Vec<f32> {
    let mut samples = Vec::new();
    for &(freq, ms, decay) in notes(choice) {
        let n = (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize;
        for i in 0..n {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.006).min(1.0);
            let env = attack * (-decay * t).exp();
            samples.push(0.5 * env * (std::f32::consts::TAU * freq * t).sin());
        }
    }
    samples
}

/// Built-in sounds are synthesized (sine + soft attack + exponential decay), so no asset files ship.
pub fn builtin(choice: SoundChoice) -> SamplesBuffer {
    SamplesBuffer::new(
        NonZeroU16::MIN,
        NonZeroU32::new(SAMPLE_RATE).unwrap_or(NonZeroU32::MIN),
        synth(choice),
    )
}

/// Reads and test-decodes a custom sound so bad files are reported when chosen,
/// not on the first chat message.
pub fn load(path: &str) -> Result<Arc<[u8]>, AudioError> {
    let err = |reason: String| AudioError::Sound {
        path: path.to_owned(),
        reason,
    };
    validate_sound(path).map_err(|e| err(e.to_string()))?;
    let len = std::fs::metadata(path)
        .map_err(|e| err(e.to_string()))?
        .len();
    if len > MAX_SOUND_BYTES {
        return Err(err("file is larger than 8 MB".into()));
    }
    let bytes: Arc<[u8]> = std::fs::read(path).map_err(|e| err(e.to_string()))?.into();
    Decoder::new(Cursor::new(bytes.clone())).map_err(|e| err(format!("cannot decode: {e}")))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_unsupported_files_error() {
        assert!(load("nope.wav").is_err());
        assert!(load("nope.flac").is_err());
    }

    #[test]
    fn builtin_sounds_have_audio_and_differ() {
        let lens: Vec<_> = [
            SoundChoice::DefaultPing,
            SoundChoice::SoftPop,
            SoundChoice::Telegram,
        ]
        .into_iter()
        .map(|c| synth(c).len())
        .collect();
        assert!(lens.iter().all(|&l| l > 1000));
        assert_ne!(lens[0], lens[1]);
        assert_ne!(lens[1], lens[2]);
    }
}
