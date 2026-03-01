# Focusrite Control 2 - Reverse Engineering Overview

## Summary

Focusrite Control 2 is a native desktop application for controlling Focusrite audio interfaces (Scarlett 4th Gen series). This document provides a comprehensive reverse engineering analysis of the application, its architecture, communication protocols, and potential extension points.

## Test Device

- **Device**: Scarlett 2i2 4th Gen
- **Product ID**: 33305 (0x8219)
- **Firmware**: 2.0.2417.0
- **Driver Version**: 4.143.0.261
- **App Version**: 1.847.0.0

## Key Findings

1. **Technology Stack**: Native C++ application built with the JUCE framework
2. **Communication Protocol**: AES70/OCA over WebSocket for remote control; proprietary "Scarlett2" USB protocol for device communication
3. **Device Discovery**: mDNS/Zeroconf for network device discovery
4. **Encryption**: libsodium secretstream for secure WebSocket connections
5. **Architecture**: Redux-like action/dispatcher pattern for state management
6. **Preset Format**: XML-based, fully human-readable and editable
7. **Direct USB**: NOT feasible on Windows — FocusriteUsb.sys claims entire device
8. **LED Halo Colors**: 4th Gen halos ARE controllable from the host — see finding 11
9. **OCA Server**: Listens on TCP 58322-58323 but requires encrypted authentication handshake
10. **USB Protocol Mapped**: 16 unique Scarlett2 protocol commands fully decoded from live USB traffic captures
11. **LED Halo Control: CONFIRMED POSSIBLE on 4th Gen** — a zlib-compressed JSON firmware schema extracted via GET_DEVMAP (initially misidentified as "AUTH_2") documents the full APP_SPACE descriptor layout, revealing LED control fields accessible via SET_DESCR + DATA_NOTIFY: `enableDirectLEDMode` (3 modes), `directLEDValues` (40 LEDs), `directLEDColour`/`directLEDIndex` (single LED), `LEDcolors` (metering gradient), `brightness` control
12. **Firmware Schema Extracted**: 24,971-byte JSON schema decoded from GET_DEVMAP data (cmd `0x000D0800`) — documents 87 APP_SPACE descriptor fields, 17 enums, 5 structs
13. **Driver Interface Paths Found**: FocusriteUsbSwRoot.sys and FocusriteUsb.sys expose device interfaces via GUID `{AC4D0455-50D7-4498-B3CD-9A41D130B759}` — the `\pal` path (SwRoot) is used by FC2 and our app, accessible via `CreateFile()` + `DeviceIoControl()`
14. **IOCTL Codes Captured**: API Monitor capture of FC2 revealed 3 IOCTL codes — `0x00222008` (general I/O), `0x00222000` (init), `0x0022200C` (notifications).
15. **FocusritePal64.dll Discovered**: FC2 communicates through `C:\Windows\System32\FocusritePal64.dll` (119KB) — a C++ library exporting `Pal::System`, `Pal::Device`, `Pal::DeviceDelegate` classes with `userDcpReceived()` callback (DCP = Scarlett2 protocol)
16. **USB Access Problem SOLVED**: TRANSACT buffer format decoded from API Monitor capture of FC2 — format is `[u64 token][u32 cmd][u32 pad][payload]`, completely different from raw Scarlett2 USB packets
17. **Driver Architecture Corrected**: Binary analysis revealed FocusriteUsbSwRoot.sys handles all control IOCTLs (not FocusriteUsb.sys). SwRoot validates sessions via `[context+0x70]` check before processing TRANSACT.
18. **4th IOCTL Discovered**: `0x00222004` found in SwRoot binary and confirmed working — returns device info
19. **BSOD Risk Documented**: Malformed IOCTL 0x222008 packets can cause kernel divide-by-zero (Bug Check 0x7E, 3 incidents). Only send well-formed packets matching FC2's exact format.
20. **TRANSACT Protocol Decoded**: FC2 API Monitor capture (282 calls) revealed the complete Windows driver protocol — session token mechanism, 35 unique command codes, initialization sequence, GET_DESCR/SET_DESCR payload formats
21. **LED Control Prototyping**: TRANSACT protocol fully working — session token comes from GET_CONFIG bytes 8-15 (not USB_INIT). GET_DESCR/SET_DESCR confirmed working. Writes to descriptor persist but LEDs don't respond visually without DATA_NOTIFY (see finding 31).
22. **Descriptor Schema Fully Decoded**: Base64+zlib compressed JSON schema (25KB) retrieved from firmware via cmd `0x000D0800`. Contains complete APP_SPACE structure with all field names, types, offsets, sizes, and notification settings.
23. **Notification Gap Identified**: `enableDirectLEDMode` has `notify-device: null` — firmware is never notified when this field changes via SET_DESCR. This likely explains why enabling direct LED mode doesn't produce visible changes. Testing alternative notification mechanisms (parameter buffer, direct color writes).
24. **LED HALO COLOR CONTROL: SOLVED**: Writing to `LEDcolors[]` (descriptor offset 384, 11 x u32, `notify-device:9`) via SET_DESCR changes all LED halo colors immediately. No `enableDirectLEDMode` or brightness change needed. Color format is `0xRRGGBB00` (RGB shifted left 8 bits). Full color cycling confirmed: RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, PURPLE, MAGENTA, WHITE all display correctly.
25. **Color Format Decoded**: `(R << 24) | (G << 16) | (B << 8)` — lowest byte unused. `0xFF000000` = RED, `0x00FF0000` = GREEN, `0x0000FF00` = BLUE, `0xFFFFFF00` = WHITE (renders with pink tint from LED hardware; visually calibrated to `0xAAFFDD00` for true white appearance). Confirmed with 9 colors.
26. **LEDcolors Semantics**: The 11-entry array is a metering gradient palette. At low/no signal, only `LEDcolors[0]` (base color) is visible. Higher indices correspond to higher signal levels.
27. **Restore Mechanism**: Writing back the original descriptor bytes `[384..428]` restores the normal metering color gradient.
28. **Direct LED Mode: FUNCTIONAL**: Requires DATA_NOTIFY(5) after SET_DESCR writes. `enableDirectLEDMode=2` + `directLEDValues` works — gives solid base color on halos. (Earlier testing without DATA_NOTIFY incorrectly concluded this was non-functional.)
29. **Button LEDs: Partially Controllable**: FC2 uses parameter buffer writes for button features, and firmware maps state to LED color internally. However, some button LEDs (Select, Output, USB) read their COLOR from `directLEDValues` even in mode=0. See finding 36.
30. **New Commands from Air Capture**: `0x00010001` (polling, 270x), `0x00010000` (channel/mixer config), `0x00020800` (**DATA_NOTIFY** — see finding 31), `0x00010004` (indices 0-4), `0x00050004`, `0x00120401`.
31. **DATA_NOTIFY Discovered**: Command `0x00020800` (raw USB `0x00800002`) is DATA_NOTIFY — sends `[event_id:u32]` to tell firmware to act on descriptor changes. This is the required activation step after SET_DESCR writes. Found via Geoffrey Bennett's Linux kernel driver source.
32. **Direct LED Control: WORKS with DATA_NOTIFY**: `enableDirectLEDMode=2` + `directLEDValues` + DATA_NOTIFY(5) gives solid per-LED colors on halos. Index mapping: 0-7 = Input 1 (number + 7 halo segments), 8-15 = Input 2 (number + 7 halo segments), 16-26 = Output halo (11 segments), 27-39 = Buttons/indicators. **Input 1 and Input 2 are independently addressable.**
33. **Per-Halo Metering Color**: `LEDcolors` with mixed gradient + DATA_NOTIFY(9) gives per-halo color based on signal level. Setting `[0]=black, [1-10]=red` makes only halos with active signal glow red — the only method for per-input-halo discrimination.
34. **Input Mute NOT POSSIBLE on 2i2 Gen4**: Linux kernel driver confirms only Vocaster devices have `INPUT_MUTE_SWITCH`. The `.mute=1` flag is a protocol encoding flag, not a user-facing control. Hardware metering cannot be suppressed.
35. **Mute Indicator Working**: Monitors Windows capture device mute state via `IAudioEndpointVolume::GetMute()`. Uses metering gradient approach: muted = red gradient + DATA_NOTIFY(9), unmuted = original gradient restored. Per-halo: only Input 1 (with mic signal) glows red.
36. **Two Categories of Button LEDs Discovered**: After exiting direct LED mode, button LEDs fall into two categories: (a) **Self-coloring** (Inst/28, 48V/29, Air/30, Safe/32, Direct/33-34,36) — parameter buffer feature toggles cause firmware to write color directly to LED hardware; descriptor values NOT updated. (b) **Cache-dependent** (Select/27+35, Auto/31, Output/37-38, USB/39) — feature toggles mark LED as "active" but read COLOR from `directLEDValues` descriptor. Must write correct default colors before toggling.
37. **DATA_NOTIFY(5) Scope in Mode=0**: `DATA_NOTIFY(5)` applies `directLEDValues` to ALL 40 LEDs (including halos), not just buttons. Stale data in halo positions (0-26) overrides the metering gradient. Must zero halo positions and only set button positions before firing NOTIFY(5), then re-apply metering gradient via NOTIFY(9).
38. **Parameter Buffer No-Op Optimization**: Firmware ignores parameter buffer writes where the new value equals the current value. To force a state change (and LED refresh), must TOGGLE: write opposite value first, then restore original. Safety: never toggle `enablePhantomPower` (notify 11, could damage condenser mics) or `preampInputGain` (notify 12, audio spikes).
39. **LED State Save/Restore Mechanism**: FocusMute uses the single-LED update mechanism (`directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8)) to color only the number indicator LEDs ("1", "2") — metering halos and all other LEDs are completely untouched. This applies to both `mute_inputs=all` (targets all number LEDs) and per-input mute (`1`, `2`, `1,2`). No LED state is saved to disk. On unmute/exit, inputs are first unmuted (so the user isn't left silently muted with no visual indication), then the firmware's native number LED colors are restored by reading `selectedInput` and sending the appropriate color (selected=green, unselected=white) via DATA_NOTIFY(8). The complex full restore sequence (mode transitions, button toggles, calibrated colors) documented in `09-led-control-api-discovery.md` was explored during prototyping but is no longer needed.
40. **directLEDValues Rendering Path Differs from Native**: The firmware's native LED rendering path (used at boot, never touched direct mode) produces different visual output than the `directLEDValues` path (used after exiting direct mode). Same descriptor value `0xFFFFFF00` appears as "off-white/too bright" through directLEDValues vs correct white natively. The directLEDValues path requires calibrated color values that compensate for the non-linear LED response — these are approximate and were determined visually during prototyping (white ≈ `0x70808800`, green ≈ `0x00380000`). **Note**: With `mute_inputs=all` (default), FocusMute only changes the metering gradient (`LEDcolors[]` + DATA_NOTIFY(9)), which uses the native rendering path where `0xFFFFFF00` displays correctly. With per-input mute (`mute_inputs=1`, `2`, `1,2`), FocusMute now uses the single-LED update mechanism (`directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8)) which targets only the number indicator LEDs — the calibration issue does not apply since it avoids the `directLEDValues` bulk path entirely.
41. **selectedInput Polling**: `selectedInput` (descriptor offset 331, u8) is readable via GET_DESCR and updates immediately on front-panel Select button press. Polling at 100ms is reliable. Value: 0=Input 1, 1=Input 2. **WARNING**: Do NOT write + DATA_NOTIFY(17) — crashes the device (requires USB unplug to recover).
42. **Firmware LED Management Confirmed**: Firmware drives number LEDs (indices 0, 8) and self-coloring buttons directly to LED hardware WITHOUT updating `directLEDValues` in the descriptor. LED colors remain 0x00000000 at those positions unless software writes them. After a DATA_NOTIFY(8) override, firmware does NOT re-assert control on number LEDs.
43. **Cache-Dependent Button Colors Confirmed**: The firmware DOES write default colors to `directLEDValues` for cache-dependent buttons (Select/27+35, Auto/31, Output/37-38, USB/39). Confirmed firmware values read from descriptor: white = `0x70808800` (R=112, G=128, B=136), green = `0x00380000` (R=0, G=56, B=0). These are ground truth, not visual approximations.
44. **Live Meter Levels via GET_METER**: METER_INFO (SwRoot 0x00000001) returns topology metadata including meter count (66 for 2i2). GET_METER (SwRoot 0x00010001) returns live 12-bit signal levels (0–4095) for all audio channels. FC2 polls at ~22.5 Hz. Channel mapping for 2i2: [0]=Analogue In 1, [1]=Analogue In 2, [2]=USB Capture 1, [3]=USB Capture 2, [4]=USB Playback 1, [5]=USB Playback 2, [10]=Analogue Out 1, [11]=Analogue Out 2, [12-65]=internal mixer bus taps (stride-7 pattern, active with Direct Monitor ON).
45. **Halo LED State Reconstructible**: With GET_METER levels + `LEDthresholds[25]` + `LEDcolors[11]`, the firmware's halo LED display can be fully reconstructed from readable data — signal level → threshold mapping → gradient color → per-segment display.
46. **IOCTL_NOTIFY — Push-Based Device Events**: IOCTL `0x0022200C` provides real-time, push-based notifications for button presses and feature toggles — no polling required. Returns 16 bytes: `[type:u32=0x20][bitmask:u32][context:u64]`. Bitmask at bytes 4-7 matches the Linux driver notification table. Confirmed: Inst (0x44000000/0x04000000), Air (0x00800000), Direct Monitor (0x01000000). Enables instant `selectedInput` detection via bit 0x02000000 instead of polling.
47. **Full Flash Dump — All 5 Segments Read**: All 1 MB of on-device flash successfully read via READ_SEGMENT (21/21 command probes succeeded). Key findings: (a) **App_Gold** (204 KB): encrypted XMOS firmware with 176-byte header (flash size, core count, entry point). No readable strings — consistent with AES encryption. (b) **App_Upgrade** (185 KB): different firmware image with a distinct header format (DFU update package, not raw XMOS boot). Also encrypted. (c) **App_Disk** (100 KB): **FAT12 filesystem** — the MSD "Easy Start" disk. Contains MBR, partition table, `SCARLETT` volume with `AUTORUN.INF`, registration URL shortcut, HTML welcome page, and Scarlett icon. This is what appears when MSD mode is enabled. (d) **App_Env** (165 bytes): plain-text key=value device metadata (serial, PCB serial, power cycles, runtime, registration URL). (e) **App_Settings** (136 KB): wear-leveling journal with 191 config snapshots showing firmware state evolution. (f) **INIT_2** returns firmware build timestamps (Dec 9 2025, 02:12:53). (g) **DRIVER_INFO** is a structured identity block with manufacturer/product/serial. (h) **IOCTL 0x222004** confirms 1024-byte max transfer size.
48. **Brightness Control: Direct Write Only**: `brightness` (offset 711, `eBrightnessMode`, 0=High/1=Medium/2=Low) is controllable only via direct write: `SET_DESCR(711, [level])` + `DATA_NOTIFY(37)`. LEDs change immediately. **Hardware-confirmed**: Parameter buffer mechanism (writing `parameterValue`/`parameterChannel`) does NOT work for brightness despite schema marking it as `set-via-parameter-buffer: true` — no visual change, descriptor stays at original value. Readback after direct write returns stale value (firmware updates descriptor asynchronously). Default is 0 (High).
49. **App_Disk FAT12 Fully Parsed**: The App_Disk segment is an MBR-partitioned disk image (not bare FAT12). MBR at offset 0 with partition table pointing to LBA 63 (0x7E00). FAT12 volume label `SCARLETT`, 321 sectors, 8 sectors/cluster. Contains 7 files: `AUTORUN.INF` (Windows autorun with icon), `CLICKHER.URL` (registration shortcut to focusrite-novation.com), `READMEFO.HTM` ("Easy Start" HTML welcome), `SCARLETT.ICO` (5-size Windows icon), `VOLUME~1.ICN` (macOS ICNS with PNG), plus two macOS resource fork artifacts. All files contain placeholder serial `00000000000000` — likely patched by firmware at runtime.
50. **inputTRSPresent is TRS-Only — Does NOT Detect XLR**: `inputTRSPresent` (offset 345, u8[2]) only detects TRS (1/4" jack) insertion via the combo jack's ring/sleeve contact sensing. XLR connections are NOT detected — field reads 0 with an XLR mic plugged in. **Hardware-confirmed on 2i2**. Push notification available via IOCTL_NOTIFY bit 0x20000000 (`FCP_NOTIFY_TRS_INPUT_CHANGE`). Implication: cannot auto-detect which inputs have mics for mute indication; config-driven `mute_inputs` remains necessary.
51. **Hardware State Snapshot Confirmed**: Read all hardware state descriptor fields on production 2i2 (fw 2.0.2417.0): `totalSecondsCounter`=2,250,322 (26d uptime, persists across power cycles), `powerCycleCounter`=245, `frontPanelSleep`=0 (disabled, factory default), `frontPanelSleepTime`=600 (10min, factory default), `usb2Connected`=0 (USB 3.x), `inputMutes`=[0,0] (no firmware-level mute, as expected).
52. **Physical Input → OS Audio Endpoint Mapping: PROVEN**: The firmware schema `device-specification` contains explicit `physical-inputs`, `physical-outputs`, `sources`, and `destinations` arrays with named router pins that define the full signal chain. For the 2i2: physical **Input 1** jack → `Analogue 1` (router-pin 128) → MUX default routing → `USB 1` (router-pin 1536) → left channel of Windows WASAPI "Analogue 1 + 2" capture endpoint. Physical **Input 2** jack → `Analogue 2` (router-pin 129) → `USB 2` (router-pin 1537) → right channel. The 2i2 has only one stereo capture endpoint bundling both inputs. Confirmed independently by three sources: (a) schema `physical-inputs` array ordering + `controls.index` values (0=Input 1, 1=Input 2), (b) GET_METER channel mapping ([0]=Analogue In 1, [1]=Analogue In 2), (c) `selectedInput` descriptor field (0=Input 1, 1=Input 2). The schema also defines `eUSB_INPUTS` enum (`eUSBInput_Input1=0`, `eUSBInput_Input2=1`) and `eANALOGUE_INPUTS` enum (`eAnalogueInput_PreampCh1=0`, `eAnalogueInput_PreampCh2=1`), making the mapping unambiguous.

## Documentation Index

### Phase 1: Binary Analysis & Architecture
- [01-technology-stack.md](01-technology-stack.md) - Framework and libraries used
- [02-architecture.md](02-architecture.md) - Application architecture and design patterns
- [03-protocol-aes70.md](03-protocol-aes70.md) - AES70/OCA communication protocol details
- [04-device-model.md](04-device-model.md) - Device capabilities and data model
- [05-actions-catalog.md](05-actions-catalog.md) - Complete catalog of all discovered actions
- [06-file-formats.md](06-file-formats.md) - Settings, presets, and log file formats

### Phase 2: Feasibility Research
- [07-direct-usb-feasibility.md](07-direct-usb-feasibility.md) - Why direct USB is blocked on Windows

### Phase 3: Live Protocol Probing
- [08-oca-probing-results.md](08-oca-probing-results.md) - OCA WebSocket server probing results + FC2 daemon XML-over-TCP IPC protocol (addendum)

### Phase 4: Firmware Schema & LED API Discovery
- [09-led-control-api-discovery.md](09-led-control-api-discovery.md) - Complete LED control API extracted from GET_DEVMAP firmware schema
- [device_firmware_schema.json](device_firmware_schema.json) - Full 24,971-byte firmware API schema (87 fields, 17 enums, 5 structs)

### Phase 5: USB Access & Driver Communication
- [10-usb-access-investigation.md](10-usb-access-investigation.md) - How to send commands through the Focusrite driver stack from userspace

### Phase 6: Driver Binary Analysis & Prototyping
- [11-driver-binary-analysis.md](11-driver-binary-analysis.md) - Binary reverse engineering of FocusriteUsbSwRoot.sys and FocusriteUsb.sys; BSOD incident; corrected driver architecture; LED control working sequence

### Phase 7: TRANSACT Protocol Decode & LED Control
- [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) - Complete TRANSACT buffer format decoded from FC2 API Monitor capture; session token mechanism; Windows driver command codes; DATA_NOTIFY breakthrough; LED control and mute indicator

### Phase 8: Protocol Reference
- [13-protocol-reference.md](13-protocol-reference.md) - Complete protocol details from Geoffrey Bennett's mixer_scarlett2.c: raw USB packet format, all command codes, SwRoot mapping, config items, notification bitmasks, parameter buffer mechanism, meter/routing/flash protocols, flash segment contents (firmware images, FAT12 filesystem, device metadata, settings journal)

### Phase 9: Firmware Binary Analysis
- [14-firmware-binary-analysis.md](14-firmware-binary-analysis.md) - XMOS XU216 firmware binary structure, encryption analysis, decryption feasibility assessment

### Phase 10: Build System & Distribution
- [15-build-and-packaging.md](15-build-and-packaging.md) - Cargo workspace structure, cross-compilation, .deb + tar.gz Linux packaging, MSI Windows installer, udev rules, CI/CD workflows, release process

### Design Notes
- [16-multi-model-mute-design.md](16-multi-model-mute-design.md) - Multi-model mute indicator architecture: design constraints for Solo, 4i4, 16i16, 18i16, 18i20 support — multiple capture endpoints, configurable MUX routing, unknown LED layouts, proposed 4-phase solution

## USB Capture Analysis

USB traffic between Focusrite Control 2 and the Scarlett 2i2 4th Gen was captured using **Wireshark + USBPcap** on Windows. The captures provided a complete picture of the proprietary Scarlett2 protocol at the USB level.

### Results

- **16 unique Scarlett2 protocol commands** fully mapped (including reads, writes, notifications, and handshakes)
- **720-byte device descriptor** decoded — contains the complete device capability map (gains, mutes, routing, phantom power, etc.)
- **Parameter buffer write mechanism** documented — how the host writes mixer/routing/gain values to the device
- **GET_DEVMAP data decoded** — initially misidentified as "AUTH_2" authentication tokens, but actually contains a **zlib-compressed JSON firmware API schema** (24,971 bytes decompressed)
- **LED halo control: CONFIRMED POSSIBLE on 4th Gen** — the firmware schema reveals a complete LED API with 40 individually-addressable LEDs, 3 control modes (off/all/halos-only), single and bulk LED control, metering gradient customization, and brightness control

See [09-led-control-api-discovery.md](09-led-control-api-discovery.md) for the LED API details and [13-protocol-reference.md](13-protocol-reference.md) for the complete USB protocol.

## Current Status

| Research Area | Status | Outcome |
|---|---|---|
| Binary analysis & architecture | COMPLETE | Full technology stack and architecture mapped |
| AES70/OCA protocol | COMPLETE | Protocol documented, server probed |
| Direct USB feasibility | COMPLETE | Blocked by FocusriteUsb.sys exclusive claim |
| LED halo research | **SOLVED** | Confirmed POSSIBLE via firmware schema hidden in GET_DEVMAP data |
| USB protocol decode | COMPLETE | 16 commands mapped, descriptor decoded, auth handshake documented |
| Firmware schema extraction | COMPLETE | 24,971-byte JSON schema decoded — 87 fields, 17 enums, 5 structs, full LED API |
| USB access investigation | **SOLVED** | TRANSACT format decoded from FC2 capture: `[u64 token][u32 cmd][u32 pad][payload]` |
| Driver binary analysis | COMPLETE | SwRoot.sys handles IOCTLs (not FocusriteUsb.sys); TRANSACT validation gate identified; sub-command table mapped |
| TRANSACT protocol decode | COMPLETE | Two captures analyzed (282 + 64 calls), SwRoot↔raw USB mapping formula decoded, complete FC2 init sequence documented |
| LED control | **SOLVED** | `LEDcolors[]` (offset 384, notify-device:9) controls halo colors via SET_DESCR + DATA_NOTIFY(9). Color format `0xRRGGBB00`. Full color cycling confirmed. |
| Direct LED mode | **SOLVED** | `enableDirectLEDMode=2` + `directLEDValues` + DATA_NOTIFY(5) works. 40 LEDs individually addressable (0-7 Input 1, 8-15 Input 2, 16-26 Output, 27-39 Buttons). |
| DATA_NOTIFY discovery | **SOLVED** | Command `0x00020800` with `[event_id:u32]` payload. Required after every SET_DESCR to activate firmware. Found via Linux kernel driver source. |
| Per-halo mute indicator | **SOLVED** | Metering gradient `[0]=black, [1-10]=red` + DATA_NOTIFY(9). Only halos with signal glow red. Monitors system mic mute state. |
| Input mute on device | **NOT POSSIBLE** | 2i2 Gen4 has no input mute — only Vocaster devices. Hardware metering cannot be suppressed. |
| Button LED control | **PARTIAL** | Two categories: self-coloring (Inst, 48V, Air, Safe, Direct) are firmware-driven; cache-dependent (Select, Output, USB, Auto) read color from `directLEDValues`. Colors controllable but rendering path differs from native. |
| LED state save/restore | **SOLVED** | No disk state needed. Number LEDs colored via DATA_NOTIFY(8) on mute; inputs unmuted and LEDs restored to firmware colors (selected=green, unselected=white) on exit. |
| Live meter levels | **SOLVED** | GET_METER returns 12-bit signal levels for all channels at up to 22.5 Hz. Meter index map confirmed for 2i2. |
| LED state readback | **PARTIAL** | Halo LED state reconstructible from GET_METER + thresholds + gradient. Button state inferable from feature flags. Firmware-driven LED colors not directly readable. |
| IOCTL_NOTIFY | **SOLVED** | Push-based device event notifications via IOCTL 0x22200C. Bitmask at bytes 4-7 of 16-byte response. Instant detection of button presses (Inst, Air, Direct confirmed; Select, 48V expected). |
| Build & packaging | COMPLETE | Cargo workspace, Windows MSI (cargo-wix), Linux .deb (cargo-deb) + tar.gz, udev rules, CI/CD workflows |
| Device command investigation | COMPLETE | All commands probed (21/21 succeeded). Full 1 MB flash dumped: encrypted firmware (App_Gold, App_Upgrade), FAT12 MSD filesystem (App_Disk, fully parsed — 7 files extracted), device metadata (App_Env), config journal (App_Settings) |
| Brightness control | **SOLVED** | Direct write + DATA_NOTIFY(37) works. LEDs visibly change. Parameter buffer does NOT work for brightness despite schema claims. |
| Input → OS endpoint mapping | **SOLVED** | Firmware schema `device-specification` defines full routing chain: physical inputs → router pins → MUX → USB capture channels. Input 1 = left channel, Input 2 = right channel of "Analogue 1 + 2" in Windows. |

### Resolved Research Questions

1. **LED notification gap** — SOLVED. `LEDcolors[]` (offset 384) + DATA_NOTIFY(9) works. `directLEDValues` (offset 92) + DATA_NOTIFY(5) also works.

2. **FC2 LED settings capture** — No longer needed. Working mechanism found independently.

3. **LED color format** — SOLVED. `0xRRGGBB00` = `(R << 24) | (G << 16) | (B << 8)`. Confirmed with 9 colors.

4. **`directLED*` fields** — SOLVED. Works with DATA_NOTIFY(5). Earlier failures were due to missing DATA_NOTIFY.

5. **Remaining LED features**:
   - `brightness` via parameter buffer — **CONFIRMED WORKING**. Both parameter buffer (Method A) and direct write + DATA_NOTIFY(37) (Method B) change LED brightness visually. Method A is fire-and-forget (descriptor not updated); Method B updates descriptor but readback lags.
   - Input 1 / Input 2 independent addressing — RESOLVED. They ARE independently addressable (indices 0-7 = Input 1, 8-15 = Input 2)
   - `directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8) — **CONFIRMED WORKING** on 2i2 4th Gen (earlier failure was due to incorrect write ordering; colour must be written before index)

6. **Application** — DONE. Built as native Rust app (FocusMute) with:
   - **Number LED mute indicator** — colors input number LEDs ("1", "2") on mute via DATA_NOTIFY(8). Both `mute_inputs=all` and per-input (`1`, `2`, `1,2`) use the same single-LED mechanism. Metering halos untouched.
   - **LED restore** — unmutes inputs on exit (so user isn't left silently muted), reads `selectedInput` on unmute/exit, restores firmware colors (selected=green, unselected=white) via DATA_NOTIFY(8). No disk state needed.
   - **WASAPI + PulseAudio mute detection** — monitors system capture device mute state (Windows + Linux)
   - **Configurable global hotkey** (default Ctrl+Shift+M; X11 only on Linux)
   - **Sound feedback** on mute/unmute (built-in sounds + configurable custom WAV paths)
   - **Desktop notifications** on mute/unmute (optional, via `notifications_enabled`)
   - **Hook commands** — run shell commands on mute/unmute (`on_mute_command`, `on_unmute_command`)
   - **System tray icon** on Windows (Win32) and Linux (GTK) with context menu (status, toggle, settings, reconnect, quit)
   - **Graceful no-device startup** — starts without a Scarlett device, shows "Disconnected" in tray, reconnects automatically when device appears
   - **Settings dialog** — mute color, hotkey, sound toggle, custom sound paths with browse/clear/preview, mute inputs, hook commands (Advanced section), autostart, device info (cross-platform egui/eframe)
   - **CLI tool** — `focusmute-cli` with `status`, `config`, `devices`, `monitor`, `probe`, `map`, `predict`, `descriptor`, `mute`, `unmute` subcommands; `--json` and `--config <path>` flags for scripting
   - **Unmute on exit** — inputs are automatically unmuted when focusmute quits, so the user isn't left silently muted with LEDs restored to normal

7. **Button LED color calibration** — NOT NEEDED. FocusMute's default mode (`mute_inputs=all`) uses the metering gradient approach, which uses the native firmware rendering path. Per-input mute uses the single-LED update mechanism (DATA_NOTIFY(8)) which targets only the number indicator LEDs and avoids the `directLEDValues` bulk rendering path entirely.

---
[Technology Stack →](01-technology-stack.md)
