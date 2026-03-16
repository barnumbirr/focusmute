//! Sound playback helpers for mute/unmute feedback.
//!
//! Sounds are pre-decoded at load time into raw samples, so playback only
//! needs to clone the sample buffer (no re-parsing on every mute toggle).

use std::io::Cursor;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, Player, Source};

// Embedded mute/unmute notification sounds (short beep tones).
pub(crate) const SOUND_MUTED: &[u8] = include_bytes!("../assets/muted.wav");
pub(crate) const SOUND_UNMUTED: &[u8] = include_bytes!("../assets/unmuted.wav");

/// Pre-decoded sound ready for playback via `SamplesBuffer`.
///
/// Samples are wrapped in `Arc` so cloning a `DecodedSound` (e.g. on every
/// mute toggle) is a cheap ref-count bump instead of copying the sample buffer.
pub(crate) struct DecodedSound {
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    samples: Arc<Vec<f32>>,
}

/// Decode raw WAV bytes into a `DecodedSound`.
fn decode_wav(wav_bytes: &[u8]) -> Option<DecodedSound> {
    let decoder = Decoder::new(Cursor::new(wav_bytes.to_vec())).ok()?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.collect();
    Some(DecodedSound {
        channels,
        sample_rate,
        samples: Arc::new(samples),
    })
}

/// Load and decode sound from a custom path, falling back to built-in on any error.
///
/// Returns `(decoded_sound, optional_warning)`. The warning is set when a custom
/// path was specified but the file could not be loaded (missing, invalid WAV, etc.).
pub(crate) fn load_sound_data(
    path: &str,
    fallback: &'static [u8],
) -> (DecodedSound, Option<String>) {
    let path = path.trim();
    if path.is_empty() {
        return (
            decode_wav(fallback).expect("embedded WAV must be valid"),
            None,
        );
    }
    match std::fs::read(path) {
        Ok(data) => match decode_wav(&data) {
            Some(decoded) => (decoded, None),
            None => {
                let msg = format!("{path} is not a valid WAV file, using built-in");
                log::warn!("[sound] {msg}");
                (
                    decode_wav(fallback).expect("embedded WAV must be valid"),
                    Some(msg),
                )
            }
        },
        Err(e) => {
            let msg = format!("could not read {path}: {e}, using built-in");
            log::warn!("[sound] {msg}");
            (
                decode_wav(fallback).expect("embedded WAV must be valid"),
                Some(msg),
            )
        }
    }
}

/// Play a pre-decoded sound, re-acquiring the default audio output each time.
///
/// Re-creating the `MixerDeviceSink` + `Player` on every call ensures playback always
/// targets the current default device. If the device changed since the last call
/// (headphones plugged in, Bluetooth connected, suspend/resume invalidated the
/// WASAPI endpoint), stale handles would silently swallow audio. The overhead is
/// negligible for short notification beeps on infrequent mute toggles.
///
/// The previous sink/player are replaced, which drops (and stops) any prior stream.
pub(crate) fn play_sound(
    sound: &DecodedSound,
    device_sink: &mut Option<rodio::MixerDeviceSink>,
    player: &mut Option<Player>,
    volume: f32,
) {
    match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(mixer) => {
            let new_player = Player::connect_new(mixer.mixer());
            new_player.set_volume(volume);
            let source =
                SamplesBuffer::new(sound.channels, sound.sample_rate, sound.samples.as_slice());
            new_player.append(source);
            *device_sink = Some(mixer);
            *player = Some(new_player);
            log::debug!("[sound] playback started (volume={volume:.0}%)");
        }
        Err(e) => {
            log::warn!("[sound] could not open audio output: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sounds_are_valid_wav() {
        let muted = Decoder::new(Cursor::new(SOUND_MUTED));
        assert!(muted.is_ok(), "muted.wav should be a valid WAV file");

        let unmuted = Decoder::new(Cursor::new(SOUND_UNMUTED));
        assert!(unmuted.is_ok(), "unmuted.wav should be a valid WAV file");
    }

    #[test]
    fn decode_builtin_muted_has_valid_metadata() {
        let decoded = decode_wav(SOUND_MUTED).expect("should decode");
        assert!(decoded.channels.get() > 0);
        assert!(decoded.sample_rate.get() > 0);
        assert!(!decoded.samples.is_empty());
    }

    #[test]
    fn decode_builtin_unmuted_has_valid_metadata() {
        let decoded = decode_wav(SOUND_UNMUTED).expect("should decode");
        assert!(decoded.channels.get() > 0);
        assert!(decoded.sample_rate.get() > 0);
        assert!(!decoded.samples.is_empty());
    }

    #[test]
    fn decode_invalid_wav_returns_none() {
        assert!(decode_wav(b"this is not wav data").is_none());
    }

    #[test]
    fn load_sound_data_empty_path_returns_decoded_builtin() {
        let (result, warning) = load_sound_data("", SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.channels, reference.channels);
        assert_eq!(result.sample_rate, reference.sample_rate);
        assert_eq!(result.samples.len(), reference.samples.len());
        assert!(warning.is_none());
    }

    #[test]
    fn load_sound_data_whitespace_path_returns_builtin() {
        let (result, warning) = load_sound_data("   ", SOUND_MUTED);
        assert!(result.channels.get() > 0);
        assert!(warning.is_none());
    }

    #[test]
    fn load_sound_data_missing_file_returns_builtin() {
        let (result, warning) = load_sound_data("/nonexistent/path/sound.wav", SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), reference.samples.len());
        assert!(warning.is_some(), "should warn about missing file");
    }

    #[test]
    fn load_sound_data_invalid_wav_returns_builtin() {
        let dir = std::env::temp_dir().join("focusmute_test_sound");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("not_a_wav.wav");
        std::fs::write(&path, b"this is not a wav file").unwrap();

        let (result, warning) = load_sound_data(path.to_str().unwrap(), SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), reference.samples.len());
        assert!(warning.is_some(), "should warn about invalid WAV");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_sound_data_valid_wav_returns_custom() {
        let dir = std::env::temp_dir().join("focusmute_test_sound_valid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.wav");
        std::fs::write(&path, SOUND_MUTED).unwrap();

        let (result, warning) = load_sound_data(path.to_str().unwrap(), SOUND_UNMUTED);
        // Should decode to the muted sound data, not the unmuted fallback
        let muted_ref = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), muted_ref.samples.len());
        assert!(warning.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
