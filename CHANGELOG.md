# FocusMute Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-03-01

### Added

- `--config <path>` global CLI flag to load settings from a custom TOML file instead of the default location
- Hotkey syntax validation in settings dialog — invalid hotkey strings (e.g. "Ctrl+Blah") now show an error before saving
- "Advanced" collapsible section in settings dialog exposing hook command fields (`on_mute_command`, `on_unmute_command`)
- Sound preview "Play" buttons in settings dialog — preview mute/unmute sounds without closing the dialog
- Sound path "Clear" buttons in settings dialog — clear a custom sound path to revert to the built-in sound
- "(built-in)" hint text on empty sound path fields in settings dialog
- `SoundPreviewPlayer` for lazy-initialized audio playback in the settings dialog
- `build_and_validate_config()` pure function extracted from settings dialog save logic for testability (7 unit tests)
- Fatal tray errors displayed as a Windows MessageBox (tray binary has no console)
- Audio poll thread death detection — logs error once if the background mute polling thread stops unexpectedly

### Changed

- Settings dialog "Mute Indicator" section: "Mute Inputs" row now appears above "Mute Color" row
- Settings dialog "Mute Color" text field now fills the full width of the section (right-to-left layout)
- Settings dialog sound rows now fill the full width of the section (right-to-left layout, no fixed button width budget)
- Unmute all inputs on exit — when FocusMute quits, inputs are unmuted so the user isn't left silently muted with no LED indication (applies to both tray app and CLI monitor)
- `set_muted()` errors in tray hotkey and menu toggle handlers now logged as warnings instead of silently ignored

### Fixed

- `enumerate_devices_windows()` now populates device serial by calling the extracted `find_usb_serial()` (previously always returned empty serial in device enumeration)

## [0.3.0] - 2026-02-28

### Added

- `--verbose` / `-v` global CLI flag for debug-level logging
- Config `save_to()` / `load_from()` methods for arbitrary file paths
- Config `load_with_warnings()` method returns parse errors as warnings instead of silently falling back to defaults
- `Config::log_path()` for platform-specific log file location
- Tray app logs to `focusmute.log` in the config directory (info level by default)
- Startup config validation with desktop notification — shows parse errors and validation warnings (invalid colors, out-of-range inputs) as a notification on launch, regardless of `notifications_enabled` setting
- `input_colors` validation in `Config::validate()` — catches invalid color values, out-of-range keys, and non-numeric keys
- Hook command RAII guard (`HookGuard`) — ensures `HOOK_RUNNING` flag is reset even if the hook thread panics
- CLI integration tests for all subcommands (devices, status, mute/unmute/descriptor/probe/monitor/map --help)
- Hook command execution tests (mute and unmute dispatch with marker file verification)

### Changed

- Split `tray/state.rs` (1249 LOC) into submodules: `state/mod.rs`, `state/icon.rs`, `state/menu.rs`, `state/hotkey.rs`
- `Config::save()` now delegates to `Config::save_to()` (DRY refactor)
- `Config::load()` now delegates to `Config::load_with_warnings()` (DRY refactor)
- `CONFIG_HEADER` constant hoisted to module level and shared between save methods

### Fixed

- Fixed stale "all (gradient mode)" display string in `MuteInputs::All` — now shows "all"
- Fixed misleading "TeamSpeak-style" comment on embedded notification sounds

## [0.2.0] - 2026-02-28

### Added

- Graceful no-device startup — tray app starts without a Scarlett device connected, shows "Disconnected" status in tray menu, and automatically connects when the device is plugged in. Hotkey, sound feedback, and notifications all work while disconnected; LED writes become no-ops until a device appears.

### Changed

- Consolidated tray menu — removed "Sound Feedback" and "Start with Windows/System" toggles (both accessible via Settings dialog) and standalone About dialog (device info moved into Settings)
- Improved settings dialog styling — grouped sections with frames, consistent button styling, section header typography, device info section
- Tuned unselected input LED white color (`0x88FFFF00` → `0xAAFFDD00`) to visually match firmware appearance on hardware

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
