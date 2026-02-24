# Driver Binary Analysis — FocusriteUsbSwRoot.sys & FocusriteUsb.sys

## Summary

Binary analysis of the two Focusrite kernel drivers corrected our understanding of the driver architecture. **FocusriteUsbSwRoot.sys** (not FocusriteUsb.sys) handles all control IOCTLs (init, transact, notify). FocusriteUsb.sys handles USB audio streaming, DFU, and DCP-over-USB transport only.

## Critical Safety Finding

**Sending malformed IOCTL 0x222008 packets to the driver causes BSOD.**

During early prototyping, brute-force scanning of driver sub-commands (1-13 with empty payloads) caused two consecutive Blue Screens of Death:

| Detail | Value |
|--------|-------|
| Bug Check | `0x0000007E` (SYSTEM_THREAD_EXCEPTION_NOT_HANDLED) |
| Exception | `0xC0000094` (STATUS_INTEGER_DIVIDE_BY_ZERO) |
| Faulting driver | FocusriteUsbSwRoot.sys |
| Minidumps | `C:\WINDOWS\Minidump\021426-11515-01.dmp`, `021426-11218-01.dmp` |
| Root cause | Some TRANSACT sub-command handlers divide by a payload-derived value; zero-length payloads trigger division by zero in kernel mode |

**Rule: NEVER send IOCTL 0x222008 without a properly-formed payload. Only use known-good Scarlett2 packet formats.**

## Architecture Correction

Previous documentation (doc 11) stated FocusriteUsb.sys handles the control IOCTLs. This is **incorrect**. The corrected architecture:

```
Focusrite Control 2.exe
  │
  ├─ FocusritePal64.dll (PAL = Platform Abstraction Layer)
  │    Pal::System → SetupDI device discovery
  │    Pal::Device → CreateFile + DeviceIoControl
  │
  v
FocusriteUsbSwRoot.sys (122KB) — SOFTWARE ROOT ENUMERATOR
  │  Creates child PDOs for audio and ASIO
  │  Handles ALL control IOCTLs:
  │    0x222000 (INIT), 0x222004 (unknown),
  │    0x222008 (TRANSACT), 0x22200C (NOTIFY)
  │  Exposes \pal device path
  │  Routes DCP commands down to FocusriteUsb.sys via internal interface
  │
  v
FocusriteUsb.sys (186KB) — USB FUNCTION DEVICE OBJECT
  │  Claims entire USB device (VID:1235 PID:8219)
  │  Exposes \usbdev device path
  │  Handles: USB audio streaming, DFU firmware updates,
  │           DCP-over-USB transport (class-specific control transfers)
  │  Does NOT handle IOCTLs 0x222000/0x222008/0x22200C
  │
  v
FocusriteUsbAudio.sys (113KB) — WDM audio miniport
```

### Key Insight

FC2 opens `\pal` (SwRoot) — not `\usbdev` (FocusriteUsb.sys). SwRoot receives TRANSACT commands via IOCTL 0x222008, validates them, and forwards DCP commands to FocusriteUsb.sys which translates them into USB control transfers.

## FocusriteUsbSwRoot.sys — Detailed Analysis

### File Details

| Property | Value |
|----------|-------|
| Size | 122,112 bytes |
| Type | Windows kernel driver (KMDF) |
| Location | `C:\Windows\System32\drivers\` |
| INF | FocusriteCustom.inf |

### IOCTL Dispatch (File Offset 0xFD95)

The IOCTL dispatch handler processes four codes:

| IOCTL | Handler | Description |
|-------|---------|-------------|
| `0x00222000` | Init handler | Returns 16-byte version/identity. Synchronous. Always succeeds. |
| `0x00222004` | Capability probe | Returns 16 bytes: `[status:u32=1][max_transfer:u32=1024][reserved:u64=0]`. Confirmed working. |
| `0x00222008` | TRANSACT handler (0x10051) | Main command channel. Async/overlapped. Contains validation + sub-command dispatch. |
| `0x0022200C` | NOTIFY handler | Pending IRP pattern — pends until device sends interrupt notification. |

### TRANSACT Handler (0x10051)

The TRANSACT handler performs validation before processing:

1. **Device context check** at offset 0x9360: reads `[device_context + 0x70]` — must be non-zero
2. If validation passes, extracts sub-command from input buffer
3. Sub-command dispatch via jump table

### Sub-Command Table

From binary analysis, the TRANSACT handler supports these sub-commands:

| Sub-cmd | Description (inferred) |
|---------|----------------------|
| 1 | Unknown |
| 2 | Unknown |
| 3 | Unknown |
| 4 | Unknown |
| 5 | Unknown |
| 6 | Unknown |
| 7 | Unknown |
| 8 | Unknown |
| 9 | "DCP transact" — forwards Scarlett2 packets to USB device |
| 10 | Unknown |
| 11 | Unknown |
| 12 | Unknown |
| 13 | Unknown |
| 500 | Unknown (special case) |

Sub-command 9 is the most relevant — it appears to be the DCP passthrough that forwards Scarlett2 protocol packets to the USB device via FocusriteUsb.sys.

### Internal Input Format (SwRoot Layer) — RESOLVED

> **Update**: API Monitor capture of FC2 (see [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md)) revealed the actual TRANSACT input format. The binary analysis guesses below were incorrect.

The actual TRANSACT input format used by FC2:

```
Offset  Size  Field
------  ----  -----
0       8     Session token (u64 LE) — 0 for first two calls, then kernel-assigned
8       4     Command code (u32 LE) — Windows driver command (NOT Scarlett2 USB cmd)
12      4     Padding (u32 LE) — always 0
16+     var   Command-specific payload
```

The earlier binary analysis misidentified the field boundaries (interpreting `0x0400` from command bytes as a "protocol marker" and subsequent bytes as "sub-command IDs"). The sub-command table identified in the binary (1-13, 500) likely represents internal dispatch within SwRoot after it parses the command code, not values that appear directly in the input buffer.

FC2 does NOT send raw Scarlett2 USB packets. FocusritePal64.dll translates application-level operations into Windows driver commands before calling DeviceIoControl.

### Strings Found in Binary

LED-related AppSpace member names found in the SwRoot binary:

```
haloColours
setGainHaloColour
getGainHaloColour
ledBrightness
ledSleepEnabled
```

Other notable strings:

```
/structs/APP_SPACE/parameter-buffer
FocusriteUsbSwRoot
DcpTransact
```

### Validation Gate: `[context+0x70]`

The TRANSACT handler calls a validation function at 0x9360 that checks `[device_context + 0x70]`. When this value is 0 (NULL), the handler rejects the TRANSACT call.

Possible interpretations:
- This field is set when the USB device is fully enumerated and ready
- This field is set during session establishment (perhaps by a preceding IOCTL 0x222004?)
- FC2 may perform a step we haven't captured that initializes this context field

## FocusriteUsb.sys — Detailed Analysis

### File Details

| Property | Value |
|----------|-------|
| Size | 186,112 bytes |
| Type | Windows kernel driver (KMDF 1.25) |
| Version | 4.143.0.261 |
| Location | `C:\Windows\System32\drivers\` |

### Responsibilities

- Claims the entire USB device (non-composite mode)
- Manages USB audio streaming (isochronous endpoints)
- Handles DFU (Device Firmware Update) protocol
- Handles DCP (Device Control Protocol) over USB — translates to class-specific control transfers
- Exposes `\usbdev` device path

### Does NOT Handle Control IOCTLs

Binary analysis confirmed FocusriteUsb.sys does **not** contain handlers for IOCTLs `0x222000`, `0x222008`, or `0x22200C`. These are exclusively handled by FocusriteUsbSwRoot.sys.

### Internal Magic Constants

Found in binary:

| Magic | ASCII | Purpose |
|-------|-------|---------|
| `0x55636F46` | `FocU` | Focusrite USB signature |
| `0x53425355` | `USBS` | USB status marker |
| `fcp1` | — | FCP version 1 |
| `fcp3` | — | FCP version 3 |
| `fcp5` | — | FCP version 5 |

### DCP Protocol

FocusriteUsb.sys implements the DCP (Device Control Protocol) transport:
- Receives DCP commands from SwRoot via internal interface
- Wraps them in USB class-specific control transfers (bRequest=0x00, wIndex based on command)
- Sends to USB endpoint and returns response

### HSM State Machine

The driver contains a Hierarchical State Machine (HSM) for managing device lifecycle states:
- Device attachment/detachment
- Audio streaming start/stop
- DFU mode transitions
- Error recovery

## FC2 Software Architecture (from DLL/Driver Analysis)

The full software stack from FC2 to USB:

```
FC2 Application Layer
  │  Redux-like action/dispatcher pattern
  │  OCA object model
  │
  ├─ FCP Layer (Focusrite Control Protocol)
  │    Maps OCA properties to descriptor offsets
  │    Handles parameter buffer write sequences
  │
  ├─ PAL Layer (FocusritePal64.dll)
  │    Pal::System — device discovery
  │    Pal::Device — IOCTL communication
  │    Formats TRANSACT commands
  │
  ├─ DCP Layer (kernel: SwRoot + FocusriteUsb.sys)
  │    SwRoot: IOCTL dispatch, validation, session management
  │    FocusriteUsb: USB transport, control transfers
  │
  └─ USB Hardware (Scarlett 2i2 4th Gen)
       Firmware processes Scarlett2 commands
       Writes to APP_SPACE descriptor fields
```

## Open Questions (Resolved)

1. **IOCTL 0x222004** — ANSWERED: Confirmed working. Returns 16 bytes: `[status:u32=1][max_transfer:u32=1024][reserved:u64=0]`. A capability/version handshake — status=1 means device present, max_transfer=1024 explains the 1024-byte page limit for READ_SEGMENT and GET_DEVMAP. Not part of the critical path for TRANSACT.

2. **How does `[context+0x70]` get set?** — LIKELY ANSWERED: The USB_INIT command (`cmd=0x00010400, token=0`) is the first TRANSACT call FC2 makes. This likely initializes the session context, setting `[context+0x70]` to the session token.

3. **Does FocusritePal64.dll transform packets?** — ANSWERED: Yes. The DLL translates application operations into the Windows driver protocol: `[u64 token][u32 cmd][u32 pad][payload]`. It does NOT pass raw Scarlett2 USB packets through. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md).

4. **Can two processes use the device simultaneously?** — **Yes, confirmed.** The focusmute app works alongside FC2 without issues. Each process gets its own session token via GET_CONFIG.

5. **What is sub-command 500?** — Still unknown. May correspond to one of the 35 Windows driver command codes identified in the capture.

## Research Prototype History

LED halo color control was achieved during prototyping and fully confirmed with color cycling. Direct LED mode was initially thought non-functional but later confirmed WORKING after discovering DATA_NOTIFY. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for full details.

> **Note**: The research prototype was never committed to this repository. It was superseded by the Focusmute app.

| Iteration | Status | Notes |
|-----------|--------|-------|
| 1 (initial) | TRANSACT fails | "Incorrect function" — wrong input format (raw Scarlett2 packets) |
| 2 (brute-force) | **CAUSED BSOD x2** | Brute-forced sub-commands 1-13 with empty payloads |
| 3 (safe) | TRANSACT fails | Same wrong format, just safer approach |
| 4 (probing) | TRANSACT fails | Discovered IOCTL 0x222004 works; SwRoot envelope gave different error (0x054F) |
| 5 (subcmd scan) | **CAUSED BSOD** | Probed SwRoot envelope subcmds 1-8 with 4-byte payloads |
| 6 (safe-only) | Works (INIT only) | Disabled IOCTL 0x222008 entirely; only INIT + INFO |
| **7 (correct format)** | TRANSACT works! | Correct format, but session token extracted from wrong place (USB_INIT, which returns all zeros) |
| **8 (token fix)** | GET_DESCR works! | Session token from GET_CONFIG bytes 8-15. Full descriptor read succeeds. SET_DESCR succeeds but LEDs don't change. |
| 9 (diagnostic) | Descriptor dumped | Full 720-byte descriptor hex dump. Writes to offset 77 persist, but writes to offset 92 were same value (no visible diff). |
| 10 (schema+color) | Color writes confirmed | Writing GREEN to offset 92 shows bytes changed. But still no visual LED change. |
| **11 (schema decode)** | **Schema fully decoded** | 25KB JSON with all field names, types, offsets. `enableDirectLEDMode` has `notify-device: null` — firmware not notified! |
| 12 (multi-test) | Testing 5 approaches | Tests brightness, LEDcolors, direct LED, single LED, and parameter buffer mechanisms |
| 13-14 | Incremental | Intermediate iterations testing various approaches |
| **15 (LEDcolors)** | **LED CONTROL ACHIEVED** | Writing `LEDcolors[]` (offset 384, 11 x u32, notify-device:9) via SET_DESCR changes halo colors. First visual LED change! |
| 16 (stdin fix) | Fixed input handling | Replaced `stdin().read(&mut [0u8])` with `read_line()` to properly consume `\r\n` on Windows. Tests run correctly one at a time. |
| **17 (color cycling)** | **FULLY CONFIRMED** | Cycles through 9 colors: RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, PURPLE, MAGENTA, WHITE. All display correctly. White has slight pink tint (LED hardware). Restores original metering gradient on exit. |
| 18 (LED dump + calibration) | Diagnostic | LED value dump tool. White calibration: pure white `0xFFFFFF00` has slight pink tint on LED hardware. |
| **19 (direct LED exhaustive)** | Initially failed, **CORRECTED in iteration 23** | Exhaustive test without DATA_NOTIFY — no visual change. Iteration 23 proved the failures were due to missing DATA_NOTIFY after SET_DESCR. With DATA_NOTIFY(5), directLEDValues works. See doc 13. |

### Working LED Control Sequence

1. Opens `\pal` with `FILE_FLAG_OVERLAPPED`
2. IOCTL 0x222000 (INIT) — synchronous
3. TRANSACT: USB_INIT (`cmd=0x00010400, token=0`)
4. TRANSACT: GET_CONFIG (`cmd=0x00040400`) → session token from bytes 8-15
5. TRANSACT: GET_DESCR (`cmd=0x00000800`) → read full descriptor, **save bytes [384..428] for restore**
6. TRANSACT: SET_DESCR (`cmd=0x00010800, offset=384, len=44`) → write 11 x `0xRRGGBB00` to `LEDcolors[]`
7. Halos change color immediately (no enableDirectLEDMode needed)
8. On exit: SET_DESCR → write back saved bytes [384..428] to restore normal metering gradient

### Color Format

```
0xRRGGBB00 = (R << 24) | (G << 16) | (B << 8)

RED     = 0xFF000000
GREEN   = 0x00FF0000
BLUE    = 0x0000FF00
WHITE   = 0xFFFFFF00
```

This sequence is implemented in Focusmute's device communication layer (`crates/focusmute-lib/src/device.rs`).

---
[← USB Access Investigation](10-usb-access-investigation.md) | [Index](README.md) | [TRANSACT Protocol →](12-transact-protocol-decoded.md)
