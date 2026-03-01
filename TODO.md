# FocusMute Roadmap

## Future

- [ ] **Multi-device support** — Support multiple Scarlett devices simultaneously. Requires per-device strategies with shared mute state, per-device reconnect backoff, config changes (`device_serials: Vec<String>` or auto-discover), CLI `--device <serial>` flag, and refactoring the single-device assumptions throughout the monitor loop and TrayState.
- [ ] **Big interface support (16i16+)** — Larger Focusrite interfaces (8i6, 18i8, 18i20, Clarett+) use the Focusrite Control Protocol (FCP) over a TCP socket instead of the `\pal` HID interface. Requires reverse-engineering the FCP socket protocol, a new `FcpDevice` implementation of the `ScarlettDevice` trait, and model profiles for each device. Likely Linux-first (fcp-server available).
- [ ] **macOS support** — New `MacosAdapter` implementing `PlatformAdapter`, CoreAudio for mute monitoring, IOKit HID for device access, .dmg packaging, and code signing/notarization.
- [ ] **Schema versioning / migration** — Validate firmware schema version against known-good versions. Detect and handle schema format changes across firmware updates to prevent silent breakage.
- [ ] **Split device.rs Windows FFI into safe wrappers** — Extract SetupDi/DeviceIoControl FFI calls into a safe abstraction layer, reducing unsafe surface area and improving testability.

## Known Limitations

- **Light tray menu on Windows dark mode** — The system tray context menu always renders in light theme on Windows, even when the OS is set to dark mode. This is a Win32 platform limitation: the `muda` crate's `MenuTheme` API only affects window menu bars, not popup/context menus. The underlying Win32 API provides no documented dark mode support for system tray context menus. Tracked upstream: [tauri-apps/muda#97](https://github.com/tauri-apps/muda/issues/97).
