# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-28

### Added

- Graceful no-device startup — tray app starts without a Scarlett device connected, shows "Disconnected" status in tray menu, and automatically connects when the device is plugged in. Hotkey, sound feedback, and notifications all work while disconnected; LED writes become no-ops until a device appears.

### Changed

- Consolidated tray menu — removed "Sound Feedback" and "Start with Windows/System" toggles (both accessible via Settings dialog) and standalone About dialog (device info moved into Settings)
- Improved settings dialog styling — grouped sections with frames, consistent button styling, section header typography, device info section
- Tuned unselected input LED white color (`0x88FFFF00`) to visually match firmware appearance on hardware

### Fixed

- Fixed deprecated `assert_cmd::Command::cargo_bin` usage in integration tests (replaced with `cargo_bin_cmd!` macro)

### Infrastructure

- Added conditional Windows code signing workflow (SignPath Foundation) — guarded by `SIGNPATH_API_TOKEN` secret in release.yml

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
