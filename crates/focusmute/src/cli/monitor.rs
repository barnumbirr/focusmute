//! `monitor` subcommand — run mute indicator (monitors mic mute, changes LED color).

use std::path::Path;
use std::sync::atomic::Ordering;

use super::{
    Config, DeviceContext, MonitorAction, MuteIndicator, MuteMonitor, RUNNING, ReconnectState,
    Result, ScarlettDevice, audio, led,
};
use focusmute_lib::device::{self, open_device_by_serial};
use focusmute_lib::hooks;

/// State for the `monitor` command, created during setup.
struct MonitorCtx {
    device: Option<device::PlatformDevice>,
    indicator: MuteIndicator,
    mute_color: u32,
    reconnect: ReconnectState,
    device_serial: String,
    config: Config,
}

/// Open device, detect model, resolve strategy.
fn monitor_setup(config: &mut Config) -> Result<MonitorCtx> {
    let mute_color = led::mute_color_or_default(config);

    let device = open_device_by_serial(&config.device_serial)?;
    println!("[device] {}", device.info().path);

    let ctx = DeviceContext::resolve(&device, false)?;

    if let Some(p) = ctx.profile {
        println!(
            "[model]  {} ({} inputs, {} LEDs)",
            p.name, p.input_count, p.led_count
        );
    } else if let Some(ref pl) = ctx.predicted {
        println!(
            "[model]  {} (predicted: {} inputs, {} LEDs)",
            pl.product_name, pl.input_count, pl.total_leds
        );
    } else {
        println!("[model]  Unknown device");
    }
    let (mute_mode, strategy, warnings) = led::resolve_strategy_from_config(
        config,
        ctx.input_count(),
        ctx.profile,
        ctx.predicted.as_ref(),
    )
    .map_err(focusmute_lib::FocusmuteError::Config)?;
    for w in &warnings {
        log::warn!("[config] {w}");
    }
    println!("[config] Mute inputs: {mute_mode}");

    let indicator = MuteIndicator::new(2, false, mute_color, strategy);

    Ok(MonitorCtx {
        device: Some(device),
        indicator,
        mute_color,
        reconnect: ReconnectState::with_defaults(),
        device_serial: config.device_serial.clone(),
        config: config.clone(),
    })
}

/// Monitor main loop: poll mute state, apply LEDs, handle reconnection.
fn monitor_loop(mctx: &mut MonitorCtx, monitor: &impl MuteMonitor) {
    let initial = monitor.is_muted();
    if initial {
        // Sync the debouncer so polls don't trigger a spurious ApplyMute
        mctx.indicator.force_state(true);
        if let Some(ref dev) = mctx.device {
            let _ = mctx.indicator.apply_mute(dev);
        }
        println!(
            "  MUTED (initial) -> {}",
            led::format_color(mctx.mute_color)
        );
    } else {
        println!("  LIVE  (initial) -> normal");
    }

    while RUNNING.load(Ordering::SeqCst) {
        // Attempt reconnection if device is disconnected
        if mctx.device.is_none()
            && let Some(new_dev) = focusmute_lib::reconnect::try_reconnect_and_refresh(
                &mut mctx.reconnect,
                mctx.indicator.strategy(),
                mctx.indicator.mute_color(),
                mctx.indicator.is_muted(),
                &mctx.device_serial,
            )
        {
            println!("[device] Reconnected to {}", new_dev.info().path);
            mctx.device = Some(new_dev);
        }

        // Wait for mute change event or 250ms fallback timeout
        monitor.wait_for_change(std::time::Duration::from_millis(250));

        // Refresh cached mute state (no-op on Windows, required for PulseAudio)
        monitor.refresh();

        let muted = monitor.is_muted();
        if let Some(ref dev) = mctx.device {
            let (action, err) = mctx.indicator.poll_and_apply(muted, dev);
            if let Some(e) = err {
                log::warn!("[device] communication error: {e}");
                log::warn!("[device] will attempt reconnection...");
                mctx.device = None;
            } else {
                match action {
                    MonitorAction::ApplyMute => {
                        println!("  MUTED -> {}", led::format_color(mctx.mute_color))
                    }
                    MonitorAction::ClearMute => println!("  LIVE  -> normal"),
                    MonitorAction::NoChange => {}
                }
                hooks::run_action_hook(action, &mctx.config);
            }
        } else {
            // Still feed the debouncer even when disconnected
            mctx.indicator.update(muted);
        }
    }
}

/// Restore LED state on exit.
fn monitor_teardown(mctx: &MonitorCtx) {
    println!();
    println!("Restoring LED state...");
    if let Some(ref dev) = mctx.device {
        if let Err(e) = led::restore_on_exit(dev, mctx.indicator.strategy()) {
            log::warn!("could not restore LED state: {e}");
        }
    } else {
        log::warn!("device disconnected, cannot restore LED state");
    }
    println!("Done.");
}

pub(super) fn cmd_monitor(config_path: Option<&Path>) -> Result<()> {
    let mut config = super::load_config(config_path);
    let mute_color = led::mute_color_or_default(&config);

    // Banner
    #[cfg(windows)]
    println!("FocusMute — Monitors mic mute state via Windows audio API.");
    #[cfg(target_os = "linux")]
    println!("FocusMute — Monitors mic mute state via PulseAudio.");
    println!(
        "  Muted:   number LEDs -> {}",
        led::format_color(mute_color)
    );
    println!("  Unmuted: number LEDs restored");
    println!("Press Ctrl+C to exit (restores original state).");
    println!();

    // Device + LED setup
    let mut mctx = monitor_setup(&mut config)?;

    // Audio init
    #[cfg(windows)]
    audio::com_init()?;

    #[cfg(windows)]
    let monitor = audio::WasapiMonitor::new()?;

    #[cfg(target_os = "linux")]
    let monitor = audio::PulseAudioMonitor::new()?;

    println!("[audio]  Capture device mute monitor ready");

    #[cfg(not(any(windows, target_os = "linux")))]
    return Err(focusmute_lib::FocusmuteError::Audio(
        focusmute_lib::audio::AudioError::InitFailed(
            "Audio mute monitoring is not yet supported on this platform.".into(),
        ),
    ));

    println!("[mute]   Color: {}", led::format_color(mute_color));
    println!();
    println!("Monitoring... (Ctrl+C to stop)");

    // Main loop
    monitor_loop(&mut mctx, &monitor);

    // Unmute all inputs so the user isn't left silently muted after exit
    // (LEDs return to normal state and can no longer indicate mute).
    if monitor.is_muted() {
        match monitor.set_muted(false) {
            Ok(()) => println!("  Unmuted inputs on exit."),
            Err(e) => log::warn!("failed to unmute on exit: {e}"),
        }
    }

    // Cleanup
    monitor_teardown(&mctx);
    Ok(())
}
