# Technology Stack

## Core Framework

- **JUCE** (Jules' Utility Class Extensions) - Cross-platform C++ audio application framework
  - Used for: GUI rendering, audio device management, component system, threading
  - Evidence: Extensive `juce::` namespace references, JUCE component IDs, `juce::Colour`, `juce::Component`, `juce::FileChooser`, etc.

## Executable Details

- **Type**: PE32+ (64-bit Windows executable)
- **Size**: ~82 MB (81,818,624 bytes)
- **Target**: Windows x86-64, GUI subsystem
- **Sections**: 6 PE sections (.text, .rdata, .data, .pdata, .rsrc, .reloc)

## Libraries & Dependencies

| Library | Purpose | Evidence |
|---------|---------|----------|
| **JUCE** | UI framework, audio, threading | `juce::Component`, `juce::Colour`, `juce::FileChooser` |
| **libuv** | Async I/O, event loop | `tcp@libuv` namespace references |
| **WebSocket** | Network communication | `websocket::` namespace, frame generators |
| **libsodium** | Encryption (secretstream) | `secretstream@aes70`, `connection_encrypt`, `connection_decrypt` |
| **libpng 1.6.37** | PNG image handling | String: "libpng version 1.6.37" |
| **WinSparkle** | Auto-update framework | `WinSparkle.dll` (2.8 MB) |
| **Catch2** | Test framework (embedded) | `CATCH2_INTERNAL_TEST_*` symbols |
| **fmt** | String formatting | `fmt::` namespace references |
| **Clara** | Command-line parsing | `Clara@Catch` namespace |
| **zeroconf** | mDNS service discovery | `zeroconf::ServiceHandle`, `ServiceManager` |

## Notable: Test Code Compiled In

The production binary contains Catch2 test framework symbols (e.g., `CATCH2_INTERNAL_TEST_839`, `CATCH2_INTERNAL_TEST_846`), including mock objects (`MockActionListener`, `MockModelListener`). This means unit test code is compiled into the release binary, which is unusual and potentially useful for understanding behavior.

## Driver Stack

Located in `C:\Program Files\Focusrite\Drivers\`:

- `FocusriteUsb.sys` - USB hardware driver (186 KB) — claims device; control IOCTLs handled by SwRoot (see doc 12)
- `FocusriteUsbAudio.sys` - USB audio class driver (113 KB)
- `FocusriteUsbMidi.sys` - USB MIDI driver (59 KB)
- `FocusriteUsbSwRoot.sys` - USB software root driver (122 KB)
- `FocusriteUsbAsio.dll` / `FocusriteUsbAsio32.dll` - ASIO drivers
- `ScarlettDfu.exe` - Device Firmware Update utility
- `Focusrite Notifier.exe` - System tray notification service
- `IHelper.exe` - Installation helper
- `Scarlett MixControl.exe` - Legacy mix control app (for older Scarlett models)

---
[← Overview](README.md) | [Index](README.md) | [Architecture →](02-architecture.md)
