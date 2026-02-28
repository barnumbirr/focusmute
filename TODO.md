# Focusmute Roadmap

## v0.2.0 (done)

- [x] **Consolidated tray menu** — Removed "Sound Feedback" and "Start with Windows/System" toggles from the tray menu (both accessible via Settings dialog). Removed standalone About dialog (device info moved into Settings).
- [x] **Prettier settings window** — Grouped sections with `egui::Frame`, consistent button styling, section header typography, device info section.
- [x] **Graceful no-device startup** — App starts without a Scarlett device connected, shows "Disconnected" in tray menu, reconnects automatically when device appears. Hotkey, sound, and notifications work while disconnected; LED writes are no-ops until a device is found.
- [x] **Windows code signing** — SignPath Foundation (free for open source). Workflow pre-wired in release.yml, guarded by `SIGNPATH_API_TOKEN` secret. Enable once approved.
- [x] **LED white calibration** — Tuned `number_led_unselected` from `0xFFFFFF00` to `0x88FFFF00` to visually match firmware white on hardware.

## Known Limitations

- **Light tray menu on Windows dark mode** — The system tray context menu always renders in light theme on Windows, even when the OS is set to dark mode. This is a Win32 platform limitation: the `muda` crate's `MenuTheme` API only affects window menu bars, not popup/context menus. The underlying Win32 API provides no documented dark mode support for system tray context menus. Tracked upstream: [tauri-apps/muda#97](https://github.com/tauri-apps/muda/issues/97).

## Future

- [ ] **Multi-device support** — Support multiple Scarlett devices simultaneously. Requires per-device strategies with shared mute state, per-device reconnect backoff, config changes (`device_serials: Vec<String>` or auto-discover), CLI `--device <serial>` flag, and refactoring the single-device assumptions throughout the monitor loop and TrayState.
- [ ] **Big interface support (16i16+)** — Larger Focusrite interfaces (8i6, 18i8, 18i20, Clarett+) use the Focusrite Control Protocol (FCP) over a TCP socket instead of the `\pal` HID interface. Requires reverse-engineering the FCP socket protocol, a new `FcpDevice` implementation of the `ScarlettDevice` trait, and model profiles for each device. Likely Linux-first (fcp-server available).
- [ ] **macOS support** — New `MacosAdapter` implementing `PlatformAdapter`, CoreAudio for mute monitoring, IOKit HID for device access, .dmg packaging, and code signing/notarization.
