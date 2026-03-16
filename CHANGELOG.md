# FocusMute Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - 2026-03-16

### Added

- Push-to-talk hotkey — independent `push_to_talk_hotkey` config field (hold to unmute, release to re-mute), works simultaneously with the toggle hotkey; PTT is a no-op when already unmuted
- Push-to-talk hotkey field in settings dialog with validation (syntax check, duplicate detection)
- Configurable log level in Settings > System (error, warn, info, debug, trace); takes effect on next launch

### Changed

- `get_descriptor` uses `checked_add(8)` to guard against integer overflow on both Windows and Linux
- Schema cache writes are now atomic (temp file + rename), matching the config save pattern
- Sound samples stored in `Arc<Vec<f32>>` for cheap sharing (was `Vec<i16>`)
- Silent `.ok()` error paths now log warnings (schema extraction, layout prediction, per-input color parsing)
- `// SAFETY:` comments on all unsafe blocks (ctrl_handler, device_wndproc, dark mode transmutes, console APIs)
- `SettingsChanges` struct replaces tuple return from `handle_settings_result`

### Infrastructure

- rodio 0.20 → 0.22 (Sink→Player, OutputStream→MixerDeviceSink, i16→f32 samples, NonZero types)
- egui/eframe 0.31 → 0.33
- tray-icon 0.19 → 0.21, muda 0.15 → 0.17, global-hotkey 0.6 → 0.7
- windows 0.61 → 0.62, rfd 0.15 → 0.17

## [0.7.4] - 2026-03-14

### Added

- Disconnected tray icon — grey desaturated icon with "Disconnected" tooltip when no Scarlett device is connected
- Event-driven USB hotplug detection — uses `RegisterDeviceNotificationW` (same approach as Focusrite Control 2) for instant disconnect/reconnect detection on Windows
- First-run welcome notification with hotkey reminder and settings hint
- Settings dialog hint text on hotkey field ("e.g. Ctrl+Shift+M"), color field ("#FF0000 or red"), and tooltip on Mute Inputs
- Hotkey registration failure notification at startup and when changing hotkey in settings
- `FirmwareVersion::is_zero()` method with warning when firmware version defaults to 0.0.0.0
- `is_device_path_present()` lightweight device enumeration (PAL interface only, no serial lookup)

### Changed

- Settings dialog resizable in width only — height auto-adjusts to content (Advanced/About sections)
- Mute UI (icon, sound, notifications) suppressed when device is disconnected — audio interface is unplugged so input states are meaningless
- User-friendly error messages: "Invalid hotkey. Examples: Ctrl+Shift+M, Alt+F1" (was raw syntax error), "Settings file corrupted. Using defaults." (was TOML error detail), "Sound file too large. Maximum size is 10 MB." (was raw byte count)
- Disconnected status shows "Connect a Scarlett 4th Gen device" instead of "Disconnected"
- Init sequence IOCTLs (USB_INIT, GET_CONFIG) use 5-second timeout instead of infinite wait
- Config temp file uses PID-based name in parent directory for robust atomic writes
- Shutdown skips LED restore when app was in live state (avoids spurious IOCTL error)
- Shutdown background thread join uses 3-second timeout instead of blocking indefinitely

### Fixed

- USB response parsing no longer panics on malformed responses — returns error instead of unwrapping
- CLI monitor logs LED apply errors instead of silently discarding them

## [0.7.3] - 2026-03-13

### Changed

- Warn-level logging for previously silent failures: LED apply/clear errors, hotkey registration failures, menu construction errors, and audio output acquisition failures

## [0.7.2] - 2026-03-12

### Fixed

- Sound playback survives sleep/wake cycles — audio output stream is now re-acquired on each mute toggle instead of being held from startup, preventing silent failure when WASAPI endpoints become stale after suspend/resume

## [0.7.1] - 2026-03-09

### Fixed

- System tray context menu now respects Windows dark mode — uses undocumented `uxtheme.dll` APIs (`SetPreferredAppMode`, `FlushMenuThemes`) to opt into dark context menus, matching behavior of Chrome, Firefox, and Windows Terminal

### Infrastructure

- Dependency lockfile bumped (all patch-level updates)

## [0.7.0] - 2026-03-02

### Added

- Structured logging for debugging and issue reports: startup banner with version and config summary, device info on connect/reconnect, mute state changes, settings dialog open/save/cancel, audio monitor readiness, shutdown sequence, and hook completion (debug level)
- Device disconnect logged when communication error causes device loss in tray app
- Config file path logged at tray startup
- Consistent `[tag]` prefixes on all log messages (`[audio]`, `[cli]`, `[config]`, `[device]`, `[focusmute]`, `[hotkey]`, `[layout]`, `[mute]`, `[schema]`, `[settings]`, `[sound]`)
- Linux notification icon — `notify-rust` notifications now display the FocusMute icon (parity with Windows AUMID branding)
- `predict` code generation now emits button labels from the predicted layout instead of an empty TODO placeholder
- Testable `_with`/`_at` variants of `try_reopen`, `try_reconnect_and_refresh`, `save_cache`, and `extract_or_cached` for dependency-injected unit testing
- ~13 new tests: reconnect with mock device factory (6), schema save/load/extract caching workflow (5), code generation button labels (2)

### Changed

- Schema cache includes format version — stale caches from older versions are automatically re-extracted
- Windows FFI: RAII wrappers (`OwnedHandle`, `DevInfoHandle`) replace manual `CloseHandle`/`SetupDiDestroyDeviceInfoList` calls; three duplicated IOCTL functions unified into `ioctl_impl`
- `save_cache` delegates to `save_cache_to`; `extract_or_cached` delegates to `extract_or_cached_at` (DRY)
- Windows notifications use direct WinRT API (`ToastNotification`) instead of `notify-rust`, eliminating a cross-platform abstraction layer on Windows
- `notify-rust` dependency moved to Linux-only

### Fixed

- Settings dialog window icon now uses the same 32 px ICO entry as the tray icon — the "ff" crossbar is visible at titlebar size (previously used the 256 px PNG which lost detail when downscaled)
- Rapid mute/unmute no longer stacks toast notifications — mute-state toasts replace the previous one (Windows: WinRT `SetTag()` replacement, Linux: D-Bus `replaces_id` via `notify-rust`)
- Toast notifications on Windows are now silent (`<audio silent="true" />`) to avoid doubling with FocusMute's own sound feedback

### Infrastructure

- CI check logic extracted into reusable workflow (`check.yml`) shared by CI and Release — eliminates copy-paste drift between the two pipelines
- Fixed missing `libxkbcommon-dev` and `libegl-dev` apt packages in Release workflow (drifted from CI)
- Third-party GitHub Actions pinned by commit SHA; release tag pattern tightened to semver (`v[0-9]+.[0-9]+.[0-9]+*`)
- Clippy now lints all targets (`--all-targets`) including test code
- CI concurrency group cancels superseded runs on the same branch/PR
- Removed SignPath Foundation code signing steps from release workflow (application declined — project too early-stage for their community adoption requirements)

## [0.6.0] - 2026-03-01

### Added

- Notifications toggle in settings dialog (Settings > System > Desktop notifications)
- `config --edit` CLI flag to open config file in `$VISUAL` / `$EDITOR` (falls back to `notepad`/`vi`)
- Per-sound volume sliders in settings dialog (separate volume for mute and unmute sounds)

### Changed

- Sound volume control split into per-sound volumes (`mute_sound_volume`, `unmute_sound_volume`) with separate sliders in settings dialog
- Settings dialog hooks tooltip: curl example rendered in monospace

## [0.5.0] - 2026-03-01

### Added

- `monitor --on-mute <cmd>` and `--on-unmute <cmd>` CLI flags to override hook commands without editing the config file
- Backward-compatible loading of legacy flat config files (pre-v0.5.0)
- Sound loading warnings (missing file, invalid WAV) now surfaced in tray startup notification balloon
- `color_to_rgb` and `rgb_to_hex` public API in `led::color` (DRY consolidation from settings dialog)
- MSRV declared as Rust 1.85 in both crates
- ~20 new tests: audio concurrency, schema edge cases, LED error propagation, settings validation, CLI integration (JSON output, hook flags)

### Changed

- Config file restructured into nested TOML sections (`[indicator]`, `[keyboard]`, `[sound]`, `[system]`, `[hooks]`) — existing flat configs are automatically migrated on next save
- Rich config file header with platform-specific paths and usage notes
- Settings dialog section renamed from "Hotkey" to "Keyboard"
- Settings dialog hooks section now has a labeled header with info tooltip; labels simplified to "On mute" / "On unmute"
- "Reconnect device" tray menu item is now always enabled — shows "Reconnect device" when disconnected and "Refresh device" when connected
- Audio monitor creation failures now logged as warnings on both Windows and Linux (previously silent)
- Release CI now runs all tests (`cargo test`) instead of just lib tests (`cargo test --lib`)

### Fixed

- Hotkey re-registration no longer loses the old hotkey if the new hotkey string is invalid — new hotkey is parsed first, and if registration fails the old hotkey is re-registered as a fallback
- Settings dialog validation errors (red text) now clear when any form field changes (text, color picker, combobox, checkboxes, browse/clear buttons), and the window resizes to keep Cancel/Save buttons visible
- Tray status text no longer overwritten from "Muted" to "Live" when starting with a device already connected in muted state

## [0.4.0] - 2026-03-01

### Added

- `--config <path>` global CLI flag to load settings from a custom TOML file instead of the default location
- Hotkey syntax validation in settings dialog — invalid hotkey strings (e.g. "Ctrl+Blah") now show an error before saving
- "Advanced" collapsible section in settings dialog with "Hooks" subsection (`on_mute_command`, `on_unmute_command`) and info tooltip
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
