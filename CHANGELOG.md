# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-24

### Added

- Real-time mute indicator on Scarlett input number LEDs (configurable color, default red)
- System tray app with settings GUI (Windows and Linux)
- CLI interface (`focusmute-cli`) with `status`, `config`, `devices`, `monitor`, `probe`, `map`, `predict`, `descriptor`, `mute`, `unmute` subcommands and `--json` flag
- Global hotkey toggle (default: Ctrl+Shift+M)
- Sound feedback on mute/unmute (built-in or custom WAV)
- Desktop notifications on mute/unmute (optional)
- Auto-reconnect on device disconnect with exponential backoff
- Per-input targeting (all input number LEDs, or specific ones like "1" or "1,2")
- Per-input mute colors (different color per input via `input_colors` config)
- Hook commands on mute state change (`on_mute_command`, `on_unmute_command`)
- Device serial targeting for multi-device setups (`device_serial`)
- Full LED profile for Scarlett 2i2 4th Gen
- Schema-driven auto-discovery for other Scarlett 4th Gen devices
- `probe` command for device detection and schema extraction
- `map` command for interactive LED layout verification
- `predict` command for offline LED layout prediction from schema JSON
- TOML configuration file support
- Auto-launch on startup option
