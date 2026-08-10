//! Sound playback helpers for mute/unmute feedback.
//!
//! Sounds are pre-decoded at load time into raw samples, so playback only
//! needs to clone the sample buffer (no re-parsing on every mute toggle).

use std::io::Cursor;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, Player, Source};

/// Extra wait allowed beyond a sound's nominal duration before concluding
/// the output stream died mid-play (covers device-open jitter, WASAPI
/// engine latency, and thread scheduling).
const DRAIN_TIMEOUT_MARGIN: Duration = Duration::from_millis(500);

/// Poll interval for [`wait_for_drain`].
const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

// Embedded mute/unmute notification sounds (short beep tones).
pub(crate) const SOUND_MUTED: &[u8] = include_bytes!("../assets/muted.wav");
pub(crate) const SOUND_UNMUTED: &[u8] = include_bytes!("../assets/unmuted.wav");

/// Pre-decoded sound ready for playback via `SamplesBuffer`.
///
/// Samples are wrapped in `Arc` so cloning a `DecodedSound` (e.g. on every
/// mute toggle) is a cheap ref-count bump instead of copying the sample buffer.
#[derive(Clone)]
pub(crate) struct DecodedSound {
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    samples: Arc<Vec<f32>>,
}

impl DecodedSound {
    /// Nominal playback duration of the decoded samples.
    fn duration(&self) -> Duration {
        let frames = self.samples.len() as f64 / f64::from(self.channels.get());
        Duration::from_secs_f64(frames / f64::from(self.sample_rate.get()))
    }
}

/// Wait until `is_empty` reports the player queue drained, or `timeout` elapses.
///
/// Returns `true` if playback drained, `false` on timeout. This replaces
/// `Player::sleep_until_end()`, whose end-signal only fires when the output
/// stream polls the queued source to exhaustion — if the WASAPI session is
/// disconnected mid-play (format change, exclusive-mode grab, device
/// removal), that signal never comes and the wait would block forever while
/// holding the dead sink. A polled deadline keeps the helper thread's
/// lifetime bounded either way.
fn wait_for_drain(is_empty: impl Fn() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !is_empty() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(DRAIN_POLL_INTERVAL);
    }
    true
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

/// Play a pre-decoded sound on a short-lived background thread.
///
/// The thread opens the default audio sink, plays the sound, waits (bounded
/// by the sound's duration plus [`DRAIN_TIMEOUT_MARGIN`]) for playback to
/// drain, then drops the sink — releasing the WASAPI endpoint immediately.
/// This avoids holding a stream between mute events, which was known to
/// wedge USB DACs (e.g. Schiit Magni Unity) when other apps (games, DAWs)
/// later tried to acquire the endpoint.
///
/// Fire-and-forget: the caller does not wait for playback to finish.
/// Errors are logged and swallowed — a missed beep is never fatal.
pub(crate) fn play_sound(sound: &DecodedSound, volume: f32) {
    let sound = sound.clone();
    std::thread::Builder::new()
        .name("sound-play".into())
        .spawn(move || {
            let mixer = match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("[sound] could not open audio output: {e}");
                    return;
                }
            };
            let player = Player::connect_new(mixer.mixer());
            player.set_volume(volume);
            let source =
                SamplesBuffer::new(sound.channels, sound.sample_rate, sound.samples.as_slice());
            player.append(source);
            log::debug!("[sound] playback started (volume={:.0}%)", volume * 100.0);
            // Wait for the beep to drain, then drop `player` and `mixer` —
            // releasing the WASAPI stream so no other app can be forced to
            // evict it later. The wait is bounded: a stream lost mid-beep
            // must not leak this thread.
            let timeout = sound.duration() + DRAIN_TIMEOUT_MARGIN;
            if !wait_for_drain(|| player.empty(), timeout) {
                log::warn!(
                    "[sound] playback did not drain within {timeout:?} — output stream likely lost"
                );
            }
            drop(player);
            drop(mixer);
        })
        .ok();
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
    fn decoded_sound_duration_from_sample_count() {
        let sound = DecodedSound {
            channels: NonZeroU16::new(2).unwrap(),
            sample_rate: NonZeroU32::new(8_000).unwrap(),
            samples: Arc::new(vec![0.0; 4_000]), // 2000 frames at 8 kHz
        };
        assert_eq!(sound.duration(), Duration::from_millis(250));
    }

    #[test]
    fn builtin_sound_durations_are_sane() {
        for bytes in [SOUND_MUTED, SOUND_UNMUTED] {
            let duration = decode_wav(bytes).unwrap().duration();
            assert!(
                duration > Duration::ZERO && duration < Duration::from_secs(5),
                "unexpected builtin sound duration: {duration:?}"
            );
        }
    }

    #[test]
    fn wait_for_drain_returns_immediately_when_empty() {
        let start = Instant::now();
        assert!(wait_for_drain(|| true, Duration::from_secs(5)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn wait_for_drain_times_out_when_stream_never_drains() {
        let timeout = Duration::from_millis(50);
        let start = Instant::now();
        assert!(!wait_for_drain(|| false, timeout));
        assert!(start.elapsed() >= timeout);
    }

    #[test]
    fn wait_for_drain_completes_when_queue_empties_later() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let polls = AtomicUsize::new(0);
        assert!(wait_for_drain(
            || polls.fetch_add(1, Ordering::SeqCst) >= 3,
            Duration::from_secs(5),
        ));
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
