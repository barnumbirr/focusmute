//! Sound playback helpers for mute/unmute feedback.
//!
//! Sounds are pre-decoded at load time into raw samples, so playback only
//! needs to clone the sample buffer (no re-parsing on every mute toggle).

use std::io::Cursor;

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, Sink, Source};

// Embedded TeamSpeak-style mute/unmute sounds
pub(crate) const SOUND_MUTED: &[u8] = include_bytes!("../assets/muted.wav");
pub(crate) const SOUND_UNMUTED: &[u8] = include_bytes!("../assets/unmuted.wav");

/// Pre-decoded sound ready for playback via `SamplesBuffer`.
pub(crate) struct DecodedSound {
    channels: u16,
    sample_rate: u32,
    samples: Vec<i16>,
}

/// Decode raw WAV bytes into a `DecodedSound`.
fn decode_wav(wav_bytes: &[u8]) -> Option<DecodedSound> {
    let decoder = Decoder::new(Cursor::new(wav_bytes.to_vec())).ok()?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<i16> = decoder.collect();
    Some(DecodedSound {
        channels,
        sample_rate,
        samples,
    })
}

/// Load and decode sound from a custom path, falling back to built-in on any error.
pub(crate) fn load_sound_data(path: &str, fallback: &'static [u8]) -> DecodedSound {
    let path = path.trim();
    if path.is_empty() {
        return decode_wav(fallback).expect("embedded WAV must be valid");
    }
    match std::fs::read(path) {
        Ok(data) => match decode_wav(&data) {
            Some(decoded) => decoded,
            None => {
                log::warn!("[sound] {path} is not a valid WAV file, using built-in");
                decode_wav(fallback).expect("embedded WAV must be valid")
            }
        },
        Err(e) => {
            log::warn!("[sound] could not read {path}: {e}, using built-in");
            decode_wav(fallback).expect("embedded WAV must be valid")
        }
    }
}

/// Append a pre-decoded sound to an existing sink (non-blocking).
pub(crate) fn play_sound(sound: &DecodedSound, sink: &Sink) {
    let source = SamplesBuffer::new(sound.channels, sound.sample_rate, sound.samples.clone());
    sink.append(source);
}

/// Initialize audio output, returning the stream and sink.
///
/// Returns `(None, None)` if audio output is unavailable (e.g. headless systems).
/// This avoids the `expect()` panic that Windows previously used.
pub(crate) fn init_audio_output() -> (Option<rodio::OutputStream>, Option<Sink>) {
    match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => (Some(stream), Sink::try_new(&handle).ok()),
        Err(e) => {
            log::warn!("could not open audio output: {e}");
            (None, None)
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
        assert!(decoded.channels > 0);
        assert!(decoded.sample_rate > 0);
        assert!(!decoded.samples.is_empty());
    }

    #[test]
    fn decode_builtin_unmuted_has_valid_metadata() {
        let decoded = decode_wav(SOUND_UNMUTED).expect("should decode");
        assert!(decoded.channels > 0);
        assert!(decoded.sample_rate > 0);
        assert!(!decoded.samples.is_empty());
    }

    #[test]
    fn decode_invalid_wav_returns_none() {
        assert!(decode_wav(b"this is not wav data").is_none());
    }

    #[test]
    fn load_sound_data_empty_path_returns_decoded_builtin() {
        let result = load_sound_data("", SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.channels, reference.channels);
        assert_eq!(result.sample_rate, reference.sample_rate);
        assert_eq!(result.samples.len(), reference.samples.len());
    }

    #[test]
    fn load_sound_data_whitespace_path_returns_builtin() {
        let result = load_sound_data("   ", SOUND_MUTED);
        assert!(result.channels > 0);
    }

    #[test]
    fn load_sound_data_missing_file_returns_builtin() {
        let result = load_sound_data("/nonexistent/path/sound.wav", SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), reference.samples.len());
    }

    #[test]
    fn load_sound_data_invalid_wav_returns_builtin() {
        let dir = std::env::temp_dir().join("focusmute_test_sound");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("not_a_wav.wav");
        std::fs::write(&path, b"this is not a wav file").unwrap();

        let result = load_sound_data(path.to_str().unwrap(), SOUND_MUTED);
        let reference = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), reference.samples.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_sound_data_valid_wav_returns_custom() {
        let dir = std::env::temp_dir().join("focusmute_test_sound_valid");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.wav");
        std::fs::write(&path, SOUND_MUTED).unwrap();

        let result = load_sound_data(path.to_str().unwrap(), SOUND_UNMUTED);
        // Should decode to the muted sound data, not the unmuted fallback
        let muted_ref = decode_wav(SOUND_MUTED).unwrap();
        assert_eq!(result.samples.len(), muted_ref.samples.len());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
