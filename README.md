# Focusmute

Hotkey mute control for Focusrite Scarlett 4th Gen interfaces.

Focusmute monitors your system microphone's mute state and reflects it on your Focusrite Scarlett interface LEDs in real time. When you mute, the input number indicator LEDs ("1", "2") turn your chosen color (default: red). When you unmute, they are restored to their firmware colors (green for the selected input, white for unselected). Metering halos and all other LEDs are never touched. It runs as a system tray app on Windows and Linux with hotkey support, or as a CLI on both platforms.

## Features

- Configurable mute indicator color (any hex color or named color)
- Global hotkey toggle (default: Ctrl+Shift+M)
- Sound feedback on mute/unmute (built-in or custom WAV)
- Auto-reconnect on device disconnect (exponential backoff) and graceful startup without device
- Desktop notifications on mute/unmute (optional)
- Hook commands on mute state change (run arbitrary shell commands)
- Per-input targeting (all input number LEDs, or specific ones like "1" or "1,2")
- Per-input mute colors (different color per input number LED)
- Schema-driven multi-model support (auto-discovers unknown Scarlett 4th Gen devices)
- Device serial targeting (multi-device setups)
- Settings GUI (tray app) and TOML config file

## Supported Devices

| Device | Support |
|--------|---------|
| Scarlett 2i2 4th Gen | Full (hardcoded LED profile) |
| Scarlett Solo / 4i4 4th Gen | Auto-discovery via firmware schema extraction |
| Scarlett 16i16 / 18i16 / 18i20 4th Gen | Untested — likely works on Windows; requires unimplemented FCP Socket protocol on Linux |

The small 4th Gen models (Solo, 2i2, 4i4) use the TRANSACT/hwdep protocol which Focusmute fully implements. The big models (16i16, 18i16, 18i20) use a different communication path on Linux (FCP Socket via a daemon process). On Windows they likely work through the same SwRoot driver, but this is unverified without hardware.

The `probe` command can detect any Scarlett 4th Gen device and extract its LED layout from firmware. Use `map` to interactively verify the predicted layout.

## Installation

### Windows

Download the MSI installer or standalone `.exe` from [Releases](../../releases).

- `focusmute.exe` -- system tray app (tray icon, hotkey, settings dialog)
- `focusmute-cli.exe` -- CLI-only binary

### Linux

**Prerequisites:** `libpulse0` (PulseAudio), `libgtk-3-0` (GTK 3), `libappindicator3-1` (system tray), `libegl1` (egui rendering). PipeWire works transparently via the `pipewire-pulse` compatibility layer — ensure `pipewire-pulse` or `libpulse0` is installed.

**Debian / Ubuntu / Mint:**

```bash
sudo dpkg -i focusmute_*.deb
sudo apt-get install -f   # resolve any missing deps
```

This installs both binaries (`focusmute` tray app + `focusmute-cli`), udev rules, and desktop entries.

**Arch / Fedora / other (from tar.gz):**

```bash
sudo install -m 755 focusmute focusmute-cli /usr/local/bin/
sudo install -m 644 99-focusrite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=usb
```

See [dist/linux/README-linux.md](dist/linux/README-linux.md) for full details and desktop entry setup.

## Usage

### Tray App (Windows + Linux)

Launch `focusmute` (or `focusmute.exe` on Windows). It sits in the system tray, monitors your mic, and updates LEDs automatically. Right-click for the menu (Status, Toggle Mute, Settings, Reconnect Device, Quit). The global hotkey (default: Ctrl+Shift+M) toggles mute. If no Scarlett device is connected at startup, the app starts in "Disconnected" mode and automatically connects when the device is plugged in.

**Linux notes:** The tray app uses GTK 3. Global hotkeys work on X11; on Wayland they may not function (use the tray menu instead).

### CLI

```
focusmute-cli <command>
```

| Command | Description |
|---------|-------------|
| `monitor` | Watch mic mute state and update LEDs in real time |
| `status` | Show device, microphone, and config status (`--json`) |
| `config` | Show current configuration and file paths (`--json`) |
| `devices` | List connected Focusrite devices (`--json`) |
| `probe` | Detect device and extract firmware schema (`--dump-schema` for full JSON) |
| `map` | Interactive LED identification (lights one index at a time) |
| `predict` | Predict LED layout from a schema JSON file (no hardware needed) |
| `descriptor` | Dump raw descriptor bytes (`--offset`, `--size`) |
| `mute` | Mute the default capture device |
| `unmute` | Unmute the default capture device |

## Configuration

Config file location:
- Windows: `%APPDATA%\Focusmute\config.toml`
- Linux: `~/.config/focusmute/config.toml`

Created with defaults on first run. Example:

```toml
mute_color = "#FF0000"
hotkey = "Ctrl+Shift+M"
sound_enabled = true
autostart = false
mute_inputs = "all"
mute_sound_path = ""
unmute_sound_path = ""
device_serial = ""
on_mute_command = ""
on_unmute_command = ""
notifications_enabled = false

[input_colors]
# 1 = "#FF0000"
# 2 = "#0000FF"
```

| Setting | Default | Description |
|---------|---------|-------------|
| `mute_color` | `"#FF0000"` | Hex color or name (e.g. `"red"`, `"#00FF00"`) |
| `hotkey` | `"Ctrl+Shift+M"` | Global hotkey (tray app; X11 only on Linux) |
| `sound_enabled` | `true` | Play sound on mute/unmute |
| `autostart` | `false` | Start on login (tray app) |
| `mute_inputs` | `"all"` | Which inputs to indicate: `"all"`, `"1"`, `"2"`, `"1,2"` |
| `mute_sound_path` | `""` | Custom WAV path (empty = built-in) |
| `unmute_sound_path` | `""` | Custom WAV path (empty = built-in) |
| `device_serial` | `""` | Preferred device serial (empty = auto-select first) |
| `on_mute_command` | `""` | Shell command to run on mute (empty = disabled) |
| `on_unmute_command` | `""` | Shell command to run on unmute (empty = disabled) |
| `input_colors` | `{}` | Per-input mute colors (TOML table, e.g. `1 = "#FF0000"`) |
| `notifications_enabled` | `false` | Show desktop notification on mute/unmute |

## Architecture

### Workspace Layout

```
focusmute/
├── Cargo.toml                          Workspace root
├── LICENSE                             Apache-2.0
├── .cargo/config.toml                  Build configuration
├── .github/workflows/
│   ├── ci.yml                          Lint, test, build (Windows + Linux)
│   └── release.yml                     Tagged release with MSI + .deb
├── dist/linux/
│   ├── README-linux.md                 Linux install guide (included in tar.gz)
│   ├── 99-focusrite.rules              udev rules for USB access
│   ├── focusmute.desktop               Desktop entry (tray app)
│   ├── focusmute-cli.desktop           Desktop entry (CLI)
│   └── debian/                         Maintainer scripts (postinst)
├── crates/focusmute-lib/               Core library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      Public API re-exports
│       ├── audio.rs                    Mic mute monitoring (WASAPI / PulseAudio)
│       ├── config.rs                   TOML settings + validation
│       ├── context.rs                  Device resolution pipeline
│       ├── device.rs                   USB communication (ScarlettDevice trait)
│       ├── error.rs                    Unified error types
│       ├── hooks.rs                    Shell command hooks (on_mute/on_unmute)
│       ├── layout.rs                   LED layout prediction from schema
│       ├── models.rs                   Hardcoded device profiles
│       ├── monitor.rs                  Mute state machine (debounce + decide)
│       ├── offsets.rs                  Descriptor offset calculations
│       ├── protocol.rs                 USB protocol constants
│       ├── reconnect.rs                Exponential backoff
│       ├── schema.rs                   Firmware schema extraction
│       └── led/
│           ├── mod.rs                  LED module re-exports
│           ├── color.rs                Color parsing (#RRGGBB <-> 0xRRGGBB00)
│           ├── ops.rs                  LED device operations
│           └── strategy.rs             Mute visualization strategy
└── crates/focusmute/                   CLI + tray app
    ├── Cargo.toml                      Defines focusmute + focusmute-cli binaries
    ├── build.rs                        Embeds app icon into .exe (Windows, via winresource)
    ├── wix/                            WiX MSI installer sources
    └── src/
        ├── main.rs                     Tray app entry point (Windows + Linux)
        ├── main_cli.rs                 CLI entry point
        ├── cli/                        CLI subcommands
        │   ├── mod.rs                  Command enum, dispatch, shared helpers
        │   ├── config_cmd.rs           config subcommand
        │   ├── descriptor.rs           descriptor subcommand
        │   ├── devices.rs              devices subcommand
        │   ├── map.rs                  map subcommand
        │   ├── monitor.rs              monitor subcommand
        │   ├── mute.rs                 mute/unmute subcommands
        │   ├── predict.rs              predict subcommand
        │   ├── probe.rs                probe subcommand
        │   └── status.rs              status subcommand
        ├── icon.rs                     Embedded PNG icon + app icon helper
        ├── settings_dialog/            Settings dialog (egui / eframe)
        │   ├── mod.rs                  Shared helpers + dispatcher
        │   └── ui.rs                   Cross-platform egui UI
        ├── tray/                       System tray app
        │   ├── mod.rs                  Platform dispatcher + single-instance
        │   ├── shared.rs               Shared event loop (PlatformAdapter trait)
        │   ├── state.rs                Tray state, menu, hotkey management
        │   ├── windows.rs              Windows adapter: Win32 message pump
        │   └── linux.rs                Linux adapter: GTK event loop
        └── sound.rs                    Pre-decoded audio playback
```

### Crate Responsibilities

**focusmute-lib** is the core library. It owns all device communication, LED control, audio monitoring, configuration, and schema parsing. It has no UI dependencies and compiles on both Windows and Linux with platform-specific backends behind `#[cfg]` gates.

**focusmute** is the application layer. It provides two binaries:
- `focusmute` -- system tray app for Windows and Linux (tray icon, hotkey, settings dialog, sound playback)
- `focusmute-cli` -- cross-platform CLI with subcommands for monitoring, diagnostics, and device control

### Module Map

| Module | Responsibility | Key Types |
|--------|---------------|-----------|
| `audio` | Mic mute monitoring | `MuteMonitor` trait, `WasapiMonitor`, `PulseAudioMonitor` |
| `config` | TOML settings + validation | `Config`, `MuteInputs` |
| `context` | Device resolution pipeline | `DeviceContext` |
| `device` | USB communication | `ScarlettDevice` trait, `DeviceInfo`, `FirmwareVersion` |
| `error` | Unified errors | `FocusmuteError`, `DeviceError`, `AudioError` |
| `hooks` | Shell command hooks | `run_action_hook` |
| `layout` | LED layout prediction | `PredictedLayout`, `PredictedLed`, `Confidence` |
| `led/color` | Color parsing | `parse_color`, `format_color` |
| `led/ops` | LED device operations | `apply_mute_indicator`, `clear_mute_indicator`, `restore_on_exit` |
| `led/strategy` | Mute visualization | `MuteStrategy`, `resolve_mute_strategy` |
| `models` | Hardcoded device profiles | `ModelProfile`, `HaloRange`, `detect_model` |
| `monitor` | Mute state machine | `MuteIndicator`, `MuteDebouncer`, `MonitorAction` |
| `offsets` | Descriptor offset calculations | `DeviceOffsets` |
| `protocol` | USB protocol constants | IOCTL codes, command codes, descriptor offsets, notify IDs |
| `reconnect` | Exponential backoff | `ReconnectState` |
| `schema` | Firmware schema extraction | `SchemaConstants`, `extract_schema`, `parse_schema` |

### Data Flow

The monitor loop follows a simple poll-decide-act pipeline:

```
Audio Backend ---poll---> MuteIndicator ---action---> LED ops ---USB---> Device
  (WASAPI /                (debounce +                (single-LED via
   PulseAudio)              decide)                    DATA_NOTIFY(8))
       ^                                                    |
       |                  ReconnectState                    |
       |                  (backoff on                       |
       +--- wait_for_change / 250ms timeout ---<--- disconnect detection)
```

1. The audio backend reports mute state changes (event-driven with 250ms polling fallback).
2. `MuteIndicator` debounces the signal (2-sample threshold) and emits `ApplyMute`, `ClearMute`, or `NoChange`.
3. LED ops translate the action into USB descriptor writes and DATA_NOTIFY(8) commands targeting number indicator LEDs.
4. On communication failure, `ReconnectState` manages exponential backoff until the device reappears.
5. On exit, number LEDs are restored to firmware colors by reading `selectedInput` (green for selected, white for unselected).

### Key Design Decisions

1. **Trait-based platform abstraction.** `ScarlettDevice` and `MuteMonitor` are traits with platform-specific implementations. This keeps the core logic testable without hardware and makes adding new backends straightforward.

2. **State machine over callbacks.** `MuteIndicator` encapsulates debouncing and mute/unmute transitions as a pure state machine. It is decoupled from I/O -- the caller drives it by feeding mute samples and applying the resulting actions.

3. **Schema-driven multi-model support.** Known devices (Scarlett 2i2 4th Gen) have hardcoded `ModelProfile`s for zero-latency startup. Unknown Scarlett 4th Gen devices are discovered at runtime by extracting the firmware schema (base64 + zlib compressed JSON) and predicting the LED layout from it.

4. **Minimal LED footprint.** Mute indication uses the single-LED update mechanism (`directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8)), targeting only the number indicator LEDs ("1", "2"). Metering halos, output LEDs, and button LEDs are never touched — the device continues normal operation.

5. **Platform-abstracted tray loop.** The `PlatformAdapter` trait injects platform-specific behavior (GTK vs Win32 event pumping, PulseAudio vs WASAPI monitoring) into a shared generic event loop (`run_core<P>`), eliminating ~80% code duplication between Windows and Linux tray implementations.

## Building from Source

### Prerequisites

- Rust toolchain (stable)
- **Linux:** `libpulse-dev`, `pkg-config`, `libasound2-dev`, `libgtk-3-dev`, `libxdo-dev`, `libappindicator3-dev`, `libegl-dev`
- **Windows cross-compile:** `gcc-mingw-w64-x86-64`, `x86_64-pc-windows-gnu` target

### Build

```bash
# Linux (native)
sudo apt-get install libpulse-dev pkg-config libasound2-dev libgtk-3-dev libxdo-dev libappindicator3-dev libegl-dev
cargo build --release

# Windows (cross-compile from Linux/WSL2)
sudo apt-get install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

Output binaries:
- Linux: `target/release/focusmute`, `target/release/focusmute-cli`
- Windows: `target/x86_64-pc-windows-gnu/release/focusmute.exe`, `focusmute-cli.exe`

### Test

```bash
cargo fmt --check                  # formatting
cargo clippy -- -D warnings        # lints
cargo test                         # unit + integration tests
cargo deny check advisories        # dependency vulnerability audit (requires cargo-deny)
```

[cargo-deny](https://github.com/EmbarkStudios/cargo-deny) is not bundled with the Rust toolchain — install it separately with `cargo install cargo-deny`.

### Packaging

#### .msi

Requires [WiX Toolset v3](https://wixtoolset.org/) (`light.exe` and `candle.exe` on PATH):

```bash
cargo install cargo-wix
cargo build --release --target x86_64-pc-windows-gnu
cargo wix -p focusmute --no-build --nocapture
```

Output: `target/wix/focusmute-<version>-x86_64.msi`

The MSI installs both binaries to `Program Files\Focusmute`, adds a Start Menu shortcut, and optionally adds the install directory to PATH. The WiX source is at `crates/focusmute/wix/main.wxs`.

#### .deb

```bash
cargo install cargo-deb
cargo build --release
cargo deb -p focusmute --no-build
```

Output: `target/debian/focusmute_<version>-1_amd64.deb`

The .deb installs both binaries to `/usr/bin/`, udev rules to `/etc/udev/rules.d/`, desktop entries to `/usr/share/applications/`, and runs `postinst`/`postrm` scripts to reload udev rules. Dependencies (`libpulse0`, `libgtk-3-0`, `libappindicator3-1`, `libegl1`) are declared automatically. The deb metadata is in `crates/focusmute/Cargo.toml` under `[package.metadata.deb]`.

## License

```
Copyright 2026 Martin Simon

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

## Buy me a coffee?

If you feel like buying me a coffee (or a beer?), donations are welcome:

```
BTC : bc1qq04jnuqqavpccfptmddqjkg7cuspy3new4sxq9
DOGE: DRBkryyau5CMxpBzVmrBAjK6dVdMZSBsuS
ETH : 0x2238A11856428b72E80D70Be8666729497059d95
LTC : MQwXsBrArLRHQzwQZAjJPNrxGS1uNDDKX6
```
