//! `status` subcommand — show device and microphone status.

use std::path::Path;

use super::{
    Config, ConfigSummaryJson, DeviceContext, DeviceStatusJson, MicrophoneStatusJson, MuteMonitor,
    Result, ScarlettDevice, StatusOutput, audio, kv, kv_indent, kv_width, led, open_device, schema,
};

/// Query current microphone status. Returns None on unsupported platforms or errors.
fn get_mic_status() -> Option<MicrophoneStatusJson> {
    #[cfg(windows)]
    {
        audio::com_init().ok()?;
        let monitor = audio::WasapiMonitor::new().ok()?;
        let muted = monitor.is_muted();
        let name = monitor.device_name().map(|s| s.to_string());
        Some(MicrophoneStatusJson { muted, name })
    }
    #[cfg(target_os = "linux")]
    {
        let monitor = audio::PulseAudioMonitor::new().ok()?;
        audio::stabilize_pulseaudio(&monitor);
        let muted = monitor.is_muted();
        let name = monitor.device_name();
        Some(MicrophoneStatusJson { muted, name })
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// Collect device status from an open device.
fn collect_device_status(dev: &impl ScarlettDevice) -> DeviceStatusJson {
    let info = dev.info();
    let ctx = DeviceContext::resolve(dev, false).ok();
    let led_support = if let Some(ref ctx) = ctx {
        if let Some(p) = ctx.profile {
            Some(format!(
                "hardcoded ({} inputs, {} LEDs)",
                p.input_count, p.led_count
            ))
        } else if let Some(ref sc) = ctx.schema {
            let cached = schema::cache_path().is_some_and(|p| p.exists());
            let suffix = if cached { ", cached" } else { "" };
            Some(format!(
                "schema ({} inputs, {} LEDs, {} gradient{suffix})",
                sc.max_inputs, sc.direct_led_count, sc.gradient_count
            ))
        } else {
            None
        }
    } else {
        None
    };
    DeviceStatusJson {
        model: info.model().to_string(),
        firmware: info.firmware.to_string(),
        serial: info.serial.clone(),
        path: info.path.clone(),
        led_support,
    }
}

/// Print or serialize the status output.
fn print_status(
    device_status: Option<DeviceStatusJson>,
    mic_status: Option<MicrophoneStatusJson>,
    config: &Config,
    json: bool,
) -> Result<()> {
    let color_display = match led::parse_color(&config.mute_color) {
        Ok(val) => led::format_color(val),
        Err(_) => format!("{} (invalid)", config.mute_color),
    };
    let mute_mode = config.parse_mute_inputs();
    let config_summary = ConfigSummaryJson {
        mute_color: color_display.clone(),
        mute_inputs: mute_mode.to_string(),
        hotkey: config.hotkey.clone(),
        sound_enabled: config.sound_enabled,
        autostart: config.autostart,
    };

    if json {
        let output = StatusOutput {
            version: env!("CARGO_PKG_VERSION").to_string(),
            device: device_status,
            microphone: mic_status,
            config: config_summary,
        };
        let json_str = serde_json::to_string_pretty(&output).map_err(|e| {
            focusmute_lib::FocusmuteError::Config(format!("JSON serialization failed: {e}"))
        })?;
        println!("{json_str}");
        return Ok(());
    }

    // Human-readable output
    let w = kv_width(
        &["Version:", "Device:", "Microphone:"],
        &[
            "Model:",
            "Firmware:",
            "Serial:",
            "Path:",
            "LED support:",
            "Name:",
            "Mute color:",
            "Mute inputs:",
            "Hotkey:",
            "Sound:",
            "Autostart:",
        ],
    );

    kv("Version:", env!("CARGO_PKG_VERSION"), w);
    println!();

    match &device_status {
        Some(dev) => {
            kv("Device:", "CONNECTED", w);
            kv_indent("Model:", &dev.model, w);
            kv_indent("Firmware:", &dev.firmware, w);
            if let Some(ref serial) = dev.serial {
                kv_indent("Serial:", serial, w);
            }
            kv_indent("Path:", &dev.path, w);
            match &dev.led_support {
                Some(support) => kv_indent("LED support:", support, w),
                None => kv_indent("LED support:", "not available", w),
            }
        }
        None => {
            kv("Device:", "NOT CONNECTED", w);
        }
    }
    println!();

    match &mic_status {
        Some(mic) => {
            kv("Microphone:", if mic.muted { "MUTED" } else { "LIVE" }, w);
            if let Some(ref name) = mic.name {
                kv_indent("Name:", name, w);
            }
        }
        None => {
            kv("Microphone:", "not available", w);
        }
    }

    println!();
    println!("Config:");
    kv_indent("Mute color:", &color_display, w);
    kv_indent("Mute inputs:", &mute_mode, w);
    kv_indent("Hotkey:", &config.hotkey, w);
    kv_indent("Sound:", if config.sound_enabled { "on" } else { "off" }, w);
    kv_indent("Autostart:", if config.autostart { "on" } else { "off" }, w);

    Ok(())
}

pub(super) fn cmd_status(json: bool, config_path: Option<&Path>) -> Result<()> {
    let device_status = open_device().ok().map(|dev| collect_device_status(&dev));
    let mic_status = get_mic_status();
    let config = super::load_config(config_path);
    print_status(device_status, mic_status, &config, json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use focusmute_lib::device::mock::MockDevice;

    #[test]
    fn collect_device_status_from_mock() {
        let dev = MockDevice::new();
        let status = collect_device_status(&dev);
        assert!(!status.model.is_empty());
        assert!(!status.firmware.is_empty());
        assert!(!status.path.is_empty());
    }

    #[test]
    fn print_status_without_device_succeeds() {
        let config = Config::default();
        let result = print_status(None, None, &config, false);
        assert!(result.is_ok());
    }

    #[test]
    fn print_status_json_without_device_succeeds() {
        let config = Config::default();
        let result = print_status(None, None, &config, true);
        assert!(result.is_ok());
    }

    #[test]
    fn print_status_with_mock_device_succeeds() {
        let dev = MockDevice::new();
        let device_status = Some(collect_device_status(&dev));
        let config = Config::default();
        let result = print_status(device_status, None, &config, false);
        assert!(result.is_ok());
    }

    #[test]
    fn print_status_json_with_mock_device_succeeds() {
        let dev = MockDevice::new();
        let device_status = Some(collect_device_status(&dev));
        let config = Config::default();
        let result = print_status(device_status, None, &config, true);
        assert!(result.is_ok());
    }
}
