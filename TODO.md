# FocusMute Roadmap

## Future

- [ ] **Multi-device support** — Support multiple Scarlett devices simultaneously. Requires per-device strategies with shared mute state, per-device reconnect backoff, config changes (`device_serials: Vec<String>` or auto-discover), CLI `--device <serial>` flag, and refactoring the single-device assumptions throughout the monitor loop and TrayState.
- [ ] **Big interface support (16i16+)** — Larger Focusrite interfaces (8i6, 18i8, 18i20, Clarett+) use the Focusrite Control Protocol (FCP) over a TCP socket instead of the `\pal` HID interface. Requires reverse-engineering the FCP socket protocol, a new `FcpDevice` implementation of the `ScarlettDevice` trait, and model profiles for each device. Likely Linux-first (fcp-server available).
- [ ] **macOS support** — New `MacosAdapter` implementing `PlatformAdapter`, CoreAudio for mute monitoring, IOKit HID for device access, .dmg packaging, and code signing/notarization.

## Known Limitations

None at present.
