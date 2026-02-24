//! `mute` / `unmute` subcommands — set OS microphone mute state.

use super::{Result, audio};

pub(super) enum MuteAction {
    Mute,
    Unmute,
}

/// Set OS mute state. LED feedback and sound are handled by the tray's
/// polling loop when it is running; the CLI intentionally does not duplicate
/// that to avoid double-firing.
pub(super) fn cmd_set_mute(action: MuteAction) -> Result<()> {
    let target = match action {
        MuteAction::Mute => true,
        MuteAction::Unmute => false,
    };

    #[cfg(windows)]
    {
        use super::MuteMonitor;
        audio::com_init()?;
        audio::WasapiMonitor::new()?.set_muted(target)?;
    }

    #[cfg(target_os = "linux")]
    {
        use super::MuteMonitor;
        let monitor = audio::PulseAudioMonitor::new()?;
        audio::stabilize_pulseaudio(&monitor);
        monitor.set_muted(target)?;
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = target;
        return Err(focusmute_lib::FocusmuteError::Audio(
            focusmute_lib::audio::AudioError::InitFailed(
                "Mute control is not yet supported on this platform.".into(),
            ),
        ));
    }

    println!("Microphone: {}", if target { "MUTED" } else { "UNMUTED" });
    Ok(())
}
