# USB Access Investigation - Sending SET_DESCR Commands on Windows

## Problem

The Scarlett 2i2 4th Gen firmware has a complete LED control API (see [09-led-control-api-discovery.md](09-led-control-api-discovery.md)), but `FocusriteUsb.sys` exclusively owns the USB device. We need a way to send `SET_DESCR` (SwRoot cmd `0x00010800`) commands from our Rust application.

## Solution: IOCTL Through the Focusrite Driver Stack

**SOLVED.** API Monitor capture of FC2 revealed the complete TRANSACT protocol. The input format is `[u64 session_token][u32 command][u32 padding][payload]` — not raw Scarlett2 USB packets as initially assumed. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the full protocol decode.

**Status**: FULLY WORKING. TRANSACT format decoded, LED control achieved, mute indicator built (Focusmute app). See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the complete protocol.

## Driver Architecture

> **Corrected**: Binary analysis (see [11-driver-binary-analysis.md](11-driver-binary-analysis.md)) revealed that **FocusriteUsbSwRoot.sys handles all control IOCTLs**, not FocusriteUsb.sys as initially assumed from the API Monitor capture.

```
Focusrite Control 2.exe
  │
  ├─ FocusritePal64.dll (119KB, C:\Windows\System32\)
  │    Pal::System → device discovery via SetupDI
  │    Pal::Device → DeviceIoControl with Scarlett2 packets
  │
  v
FocusriteUsbSwRoot.sys (122KB) — Software root enumerator
  │  Handles ALL control IOCTLs: 0x222000, 0x222004, 0x222008, 0x22200C
  │  Exposes \pal device path via GUID {AC4D0455-...-B759}
  │  Validates sessions, dispatches sub-commands
  │  Forwards DCP commands to FocusriteUsb.sys
  │
  v
FocusriteUsb.sys (186KB, KMDF 1.25, v4.143.0.261)
  │  Claims entire USB device (VID:1235 PID:8219)
  │  Exposes \usbdev device path via same GUID
  │  USB audio streaming, DFU, DCP-over-USB transport
  │  Does NOT handle control IOCTLs directly
  │
  v
FocusriteUsbAudio.sys (113KB) — WDM audio miniport driver
```

### Driver Files

| File | Size | Path | Purpose |
|------|------|------|---------|
| FocusriteUsbSwRoot.sys | 122,112 | `C:\Windows\System32\drivers\` | Software root; handles control IOCTLs |
| FocusriteUsb.sys | 186,112 | `C:\Windows\System32\drivers\` | USB FDO; audio, DFU, DCP-over-USB transport |
| FocusriteUsbAudio.sys | 113,408 | `C:\Windows\System32\drivers\` | WDM audio miniport |
| FocusritePCIeSwRoot.sys | 122,112 | `C:\Windows\System32\drivers\` | Thunderbolt variant of SwRoot |
| FocusritePal64.dll | 118,808 | `C:\Windows\System32\` | Platform Abstraction Layer (userspace API) |
| FocusriteUsbAsio.dll | 92,160 | `C:\Program Files\Focusrite\Drivers\` | ASIO driver interface |

## FocusritePal64.dll — The Key Library

FC2 communicates with the driver exclusively through `FocusritePal64.dll`, a C++ shared library that exports a clean class-based API.

### Exported Classes

```cpp
namespace Pal {
    class System {
        // Factory: creates system instance with device discovery
        static shared_ptr<System> createSystem(SystemDelegate*);
    };

    class SystemDelegate {
        virtual void deviceAdded(shared_ptr<Device>);
        virtual void deviceRemoved(shared_ptr<Device>);
        virtual bool supportsUsb();
        virtual bool supportsUsbDevice(unsigned int productId);
    };

    class Device {
        // Represents a connected Focusrite device
        // Internal: opens device interface, sends IOCTLs
    };

    class DeviceDelegate {
        virtual void userDcpReceived(unsigned int);  // DCP = Scarlett2 protocol
        virtual void mount();
        virtual void unmount();
        virtual void bufferSizeChanged();
        virtual void clockLockChanged();
        virtual void clockSourceChanged();
        virtual void sampleRateChanged();
        virtual void firmwareProgress(unsigned int, FirmwareOperation);
    };
}
```

### DLL Imports

| DLL | Key Functions | Purpose |
|-----|---------------|---------|
| KERNEL32.dll | `CreateFileA`, `DeviceIoControl`, `GetOverlappedResult`, `CreateEventA`, `CloseHandle`, `CancelSynchronousIo` | Device I/O (async/overlapped) |
| SETUPAPI.dll | `SetupDiGetClassDevsA`, `SetupDiEnumDeviceInterfaces`, `SetupDiGetDeviceInterfaceDetailA`, `SetupDiOpenDeviceInterfaceRegKey` | Device discovery |
| USER32.dll | `RegisterDeviceNotificationA`, `CreateWindowExA`, `GetMessageA`, `DefWindowProcA` | Hotplug detection (hidden message window) |

### Device Discovery Pattern

```
1. SetupDiGetClassDevs(GUID={AC4D0455-50D7-4498-B3CD-9A41D130B759}, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
2. SetupDiEnumDeviceInterfaces() → iterate available devices
3. SetupDiGetDeviceInterfaceDetail() → get device path string
4. CreateFile(devicePath, GENERIC_READ | GENERIC_WRITE, FILE_FLAG_OVERLAPPED)
5. DeviceIoControl(handle, IOCTL_CODE, inputBuffer, ...) → send Scarlett2 packets
```

## Exposed Device Interface Paths

The driver registers three openable interfaces:

| Path | GUID | Purpose | Driver | IOCTL Support |
|------|------|---------|--------|---------------|
| `...#focusriteusbnew#0000#{ac4d0455-...b759}\pal` | `{AC4D0455-...B759}` | PAL interface (**FC2 uses this**) | FocusriteUsbSwRoot.sys | All 4 IOCTLs |
| `...#vid_1235&pid_8219#...#{ac4d0455-...b759}\usbdev` | `{AC4D0455-...B759}` | USB device control | FocusriteUsb.sys | None (IOCTL calls fail) |
| `...#focusriteusbnew#0000#{ac4d0455-41c6-66ba-...}\asio` | `{AC4D0455-...B759}` | ASIO interface | FocusriteUsbSwRoot.sys | Unknown |

**Security**: `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGWGX;;;WD)` — System and Admins get full access; **Everyone gets read/write/execute**.

### Hidden WinUSB Interface

USB interface 3 ("Focusrite Control") has a PnP entry with WinUSB driver, but status is "Not Present" because FocusriteUsb.sys claims the whole device as a non-composite device:
- Instance ID: `USB\VID_1235&PID_8219&MI_03\...`
- Service: WINUSB
- Status: Not Present

## API Monitor Capture Results

### Capture Details

- **Tool**: API Monitor v2 (64-bit), hooked `NtDeviceIoControlFile`
- **Target**: Focusrite Control 2.exe (PID from live session)
- **Duration**: 56 seconds (6:31:47.606 PM to 6:32:44.008 PM)
- **Total calls**: 2988 NtDeviceIoControlFile entries
- **Focusrite calls**: 2622 (1311 via FocusritePal64.dll `DeviceIoControl` + 1311 mirrored as KERNELBASE `NtDeviceIoControlFile`)
- **User actions during capture**: Two Direct Monitor toggles (on → off → on)

### BSOD Warning

> **DANGER**: Sending malformed packets via IOCTL 0x222008 can cause Blue Screen of Death. During early prototyping, brute-forcing driver sub-commands with empty payloads caused Bug Check 0x7E (divide-by-zero in kernel mode). See [11-driver-binary-analysis.md](11-driver-binary-analysis.md) for full incident details.

### Focusrite IOCTL Codes

API Monitor captured **3 IOCTL codes** in use by FC2. Binary analysis of SwRoot revealed a **4th** (0x222004) that FC2 may use in a way not captured.

All Focusrite communication uses a **single device handle** (`0x0a70`) and these IOCTL codes:

#### IOCTL 0x00222000 — Device Info / Init Handshake

```
CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)
  DeviceType:  0x0022 (FILE_DEVICE_UNKNOWN, vendor-specific)
  Function:    0x800 (2048, first vendor function)
  Method:      METHOD_BUFFERED
  Access:      FILE_ANY_ACCESS
```

| Metric | Value |
|--------|-------|
| Count | 2 (1 call, seen as both FocusritePal64 + KERNELBASE) |
| Input buffer | 0 bytes (no input) |
| Output buffer | 16 bytes |
| Return | `STATUS_SUCCESS` (synchronous — only IOCTL that returns synchronously) |
| Thread | 24 |
| Timing | First IOCTL sent, at very start of init |

This is the "who are you?" handshake — sent once at startup, returns 16 bytes of device identity/protocol version.

#### IOCTL 0x00222008 — General I/O (Reads, Writes, Meter Polling)

```
CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)
  DeviceType:  0x0022
  Function:    0x802 (2050)
  Method:      METHOD_BUFFERED
  Access:      FILE_ANY_ACCESS
```

| Metric | Value |
|--------|-------|
| Count | 2614 (1307 per layer) |
| Input buffer sizes | 16, 18, 20, **24**, 25, 28 bytes |
| Output buffer sizes | 8, 9, 12, 16, 20, 24, 32, 56, 83, 96, 221, 228, **272**, 728, 1032 bytes |
| Return | All `STATUS_PENDING` (asynchronous / overlapped I/O) |
| Threads | 23, 24, 25 (thread 25 = dedicated polling) |

**This is the workhorse IOCTL.** It serves as a multiplexed command channel — the same IOCTL code handles all reads, writes, and meter polling. The Scarlett2 protocol packet header inside the input buffer determines what command is executed.

**Dominant steady-state pattern**: `in=24, out=272` at **~21 Hz** (47ms median interval) = GET_METER polling.

#### IOCTL 0x00222004 — Capability/Version Probe (confirmed)

```
CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)
  DeviceType:  0x0022
  Function:    0x801 (2049)
  Method:      METHOD_BUFFERED
  Access:      FILE_ANY_ACCESS
```

| Metric | Value |
|--------|-------|
| Count | 0 (not seen in API Monitor capture) |
| Source | Found in SwRoot.sys IOCTL dispatch at file offset 0xFD95 |
| Purpose | Unknown — possibly session registration or device context initialization |

This IOCTL was not captured in the API Monitor session. It may be called during early device enumeration (before we attached the monitor), or it may only be used in specific scenarios. It could be the missing "session registration" step that sets `[context+0x70]` to non-zero, enabling TRANSACT to work.

#### IOCTL 0x0022200C — Notification / Interrupt

```
CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)
  DeviceType:  0x0022
  Function:    0x803 (2051)
  Method:      METHOD_BUFFERED
  Access:      FILE_ANY_ACCESS
```

| Metric | Value |
|--------|-------|
| Count | 6 (3 per layer) |
| Input buffer | 0 bytes (no input) |
| Output buffer | 16 bytes |
| Return | All `STATUS_PENDING` (async — pends until device sends notification) |
| Thread | 26 (dedicated notification thread) |

Classic pending-IRP pattern: the app submits the IOCTL, it pends in the driver until the device generates an interrupt notification, then the driver completes it and the app immediately re-submits. The 16-byte output likely matches the 8-byte interrupt endpoint bitmask from the USB capture.

### Buffer Size ↔ Windows Driver Protocol Mapping

> **Corrected**: The IOCTL input buffers use the Windows driver protocol (`[u64 token][u32 cmd][u32 pad][payload]`), NOT raw Scarlett2 USB packets. The 16-byte "header" is the token+cmd+pad, not the Scarlett2 cmd/size/seq/error/pad.

| Input Size | Windows Driver Interpretation | Header (16) + Payload |
|------------|------------------------------|----------------------|
| 16 bytes | Command with no payload (e.g., USB_INIT, GET_CONFIG) | 16 + 0 |
| 24 bytes | Command with 8-byte payload (e.g., GET_DESCR: offset:u32 + size:u32) | 16 + 8 |
| 25 bytes | SET_DESCR writing 1 byte (offset:u32 + length:u32 + value:u8) | 16 + 9 |

| Output Size | Interpretation | Header (8) + Data |
|-------------|---------------|-------------------|
| 8 bytes | Minimal response (header only, or session token) | 8 + 0 |
| 96 bytes | GET_CONFIG response | 8 + 88 |
| **272 bytes** | GET_METER response (264 bytes meter data) | 8 + 264 |
| **728 bytes** | GET_DESCR response (full 720-byte descriptor) | 8 + 720 |

### Timing Phases

#### Phase 1: Initialization (~54ms, 39 Focusrite calls)

```
seq 194:    IOCTL 0x222000, in=0,  out=16    → Device info handshake
seq 196+:   IOCTL 0x222008, in=16, out=8     → Get capabilities
seq ...:    IOCTL 0x222008, in=16, out=96     → Read config blocks (3x)
seq ...:    IOCTL 0x222008, in=18, out=9      → Read per-channel settings (8x)
seq ...:    IOCTL 0x222008, in=20, out=32     → Read mixer/routing (5x)
seq 278:    IOCTL 0x22200C, in=0,  out=16     → Subscribe to notifications
seq 280+:   IOCTL 0x222008, in=20, out=1032   → Read large blocks (5x, flash/auth?)
seq ...:    IOCTL 0x222008, in=24, out=728    → Read full descriptor (720 bytes)
```

Then a 1072ms gap for UI rendering.

#### Phase 2: Steady-State Meter Polling (55 seconds)

- 1263 calls of `IOCTL 0x222008` with `in=24, out=272`
- Thread 25 exclusively (dedicated polling thread)
- Interval: 47ms median (~21 Hz)
- All asynchronous (`STATUS_PENDING`)

#### Phase 3: User Action Bursts (Direct Monitor Toggles)

Two identical bursts detected at 6:31:53.256 PM and 6:31:55.150 PM (1.9s apart):

```
... polling (24/272, 24/272, 24/272) ...
  IOCTL 0x222008, in=25, out=8     ← SET_DESCR: write 1 byte to device
  IOCTL 0x222008, in=25, out=8     ← SET_DESCR: write 1 byte (second param)
  IOCTL 0x222008, in=20, out=8     ← Read/acknowledge
  IOCTL 0x22200C, in=0,  out=16    ← Notification fires (device confirms change)
  IOCTL 0x222008, in=24, out=9     ← Read back new parameter value
... polling resumes (24/272, 24/272) ...
```

This matches the known parameter buffer write mechanism:
1. `SET_DESCR(offset=0xFD, value=channel)` → 25-byte input
2. `SET_DESCR(offset=0xFC, value=new_value)` → 25-byte input
3. `DESCR_CMD(activate)` → 20-byte input
4. Device interrupt notification confirms change
5. Read back new state → 24-byte input, 9-byte output (1-byte value)

### Thread Architecture

| Thread | Role | IOCTLs Used |
|--------|------|-------------|
| 24 | Initialization / setup | 0x222000, 0x222008 |
| 23 | Bulk data reads during init | 0x222008 |
| 25 | Dedicated meter polling (~22.5 Hz) | 0x222008 (24→272) |
| 26 | Notification listener | 0x22200C |

### Non-Focusrite Calls (Filtered Out)

| Source | IOCTL Type | Count | Purpose |
|--------|-----------|-------|---------|
| NSI.dll | 0x120007, 0x12000F | 41 | Network stack interface |
| mswsock.dll | IOCTL_AFD_* | 152 | Socket operations (OCA server) |
| bcrypt.dll | 0x390000/0x390400 | 8 | Cryptographic operations |
| KERNELBASE.dll | IOCTL_MOUNT* | 27 | Volume/mount management |
| KERNELBASE.dll | 0x470xxx | 65 | Device property PnP queries |

## How to Send SET_DESCR From Our App

> **Updated**: The correct protocol uses Windows driver commands, not raw Scarlett2 USB packets. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the full protocol.

### Recipe

```
1. Enumerate device interfaces with SetupDiGetClassDevs:
   - GUID: {AC4D0455-50D7-4498-B3CD-9A41D130B759}
   - Flags: DIGCF_PRESENT | DIGCF_DEVICEINTERFACE

2. Find the \pal device path via SetupDiGetDeviceInterfaceDetail
   → "\\?\...#focusriteusbnew#0000#{ac4d0455-...}\pal"

3. Open with CreateFile:
   - Access: GENERIC_READ | GENERIC_WRITE
   - Flags: FILE_FLAG_OVERLAPPED (async I/O)

4. IOCTL 0x222000 (INIT): sync, no input, 16-byte output

5. TRANSACT (IOCTL 0x222008): USB_INIT
   - Input: [token=0 : u64][cmd=0x00010400 : u32][pad=0 : u32]
   - Output: 8-byte session token

6. TRANSACT: GET_CONFIG (optional)
   - Input: [token=T : u64][cmd=0x00040400 : u32][pad=0 : u32]
   - Output: 96 bytes

7. TRANSACT: SET_DESCR
   - Input: [token=T : u64][cmd=0x00010800 : u32][pad=0 : u32]
            [offset : u32][length : u32][data : N bytes]
   - Output: 8-byte acknowledgment
```

### TRANSACT Buffer Format

```rust
/// Build TRANSACT input: [u64 token][u32 cmd][u32 pad=0][payload...]
fn transact_buf(token: u64, cmd: u32, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + payload.len());
    buf.extend_from_slice(&token.to_le_bytes());
    buf.extend_from_slice(&cmd.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}
```

### Example: Enable Direct LED Mode (Halos Only)

```rust
// Step 1: USB_INIT (firmware handshake, token=0)
let init_buf = transact_buf(0, 0x00010400, &[]);
ioctl_async(handle, 0x222008, &init_buf, 100)?;

// Step 2: GET_CONFIG — session token is at response bytes 8-15
let config_buf = transact_buf(0, 0x00040400, &[]);
let config_response = ioctl_async(handle, 0x222008, &config_buf, 96)?;
let token = u64::from_le_bytes(config_response[8..16].try_into().unwrap());

// Step 3: SET_DESCR — write 0x02 to offset 77 (enableDirectLEDMode)
let mut payload = Vec::new();
payload.extend_from_slice(&77u32.to_le_bytes());  // offset
payload.extend_from_slice(&1u32.to_le_bytes());   // length
payload.push(2);                                   // value = eDirectLEDModeHalosOnly
let set_buf = transact_buf(token, 0x00010800, &payload);
ioctl_async(handle, 0x222008, &set_buf, 8)?;
```

### Example: Set All Halos to Red

```rust
// After enabling direct LED mode (as above):

// SET_DESCR: write directLEDValues (offset 92, 160 bytes)
let mut payload = Vec::new();
payload.extend_from_slice(&92u32.to_le_bytes());   // offset (directLEDValues)
payload.extend_from_slice(&160u32.to_le_bytes());  // length = 40 × 4 bytes
for _ in 0..40 {
    payload.extend_from_slice(&0xFF000000u32.to_le_bytes()); // RED = 0xRRGGBB00
}
let set_buf = transact_buf(token, 0x00010800, &payload);
ioctl_async(handle, 0x222008, &set_buf, 8)?;

// CRITICAL: Send DATA_NOTIFY to activate the firmware
let notify_payload = 5u32.to_le_bytes(); // event_id=5 for directLEDValues
let notify_buf = transact_buf(token, 0x00020800, &notify_payload);
ioctl_async(handle, 0x222008, &notify_buf, 8)?;
```

## Alternative Approach: FC2's OCA Server (Backup)

### Architecture

FC2 runs two AES70/OCA endpoints:
- `Aes70SecureEndpoint` (port 58323): WebSocket + libsodium secretstream encryption
- `Aes70InsecureEndpoint`: **Plain WebSocket, NO encryption** (for local connections)

### Encryption Key (for secure endpoint)

Found in `C:\Users\...\Focusrite Control 2\settings.xml` as a `<RemoteConnection key="...">` element.
256-bit hex key for libsodium secretstream (XChaCha20-Poly1305). The secure endpoint also requires a QR code authentication handshake.

### Limitations

- Requires FC2 to be running
- Must map OCA object properties to USB descriptor offsets
- May not expose LED control as OCA properties

## Approaches Ruled Out

| Approach | Reason |
|----------|--------|
| **WinUSB co-existence** | Device not composite-enumerated; FocusriteUsb.sys claims entire device |
| **libusb / rusb / nusb** | Cannot co-exist with proprietary driver on Windows |
| **Standard USB IOCTLs** | `IOCTL_INTERNAL_USB_SUBMIT_URB` is kernel-mode only |
| **KMDF filter driver** | Overkill: 4-10 weeks, requires EV code signing for distribution |

## Reference: GUIDs and Constants

### Device Interface GUIDs

| GUID | Purpose |
|------|---------|
| `{AC4D0455-50D7-4498-B3CD-9A41D130B759}` | Device interface (USB + PAL) |
| `{AC4D0455-41C6-66BA-B3CD-9A41D130B759}` | ASIO interface |
| `{C8B76578-D062-4834-0001-F8B6F2162A22}` | Focusrite Audio device class |
| `{343714AA-B0D8-4A51-AFBD-3BB26C8343E1}` | ASIO COM CLSID |

### IOCTL Codes

| Code (hex) | Code (decimal) | CTL_CODE | Purpose |
|------------|---------------|----------|---------|
| `0x00222000` | 2236416 | `CTL_CODE(0x22, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)` | Device info / init |
| `0x00222004` | 2236420 | `CTL_CODE(0x22, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)` | Capability probe: returns `[status:u32=1][max_transfer:u32=1024][reserved:u64=0]` |
| `0x00222008` | 2236424 | `CTL_CODE(0x22, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)` | General I/O (all commands) |
| `0x0022200C` | 2236428 | `CTL_CODE(0x22, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)` | Notification listener |

### Source Code Paths (from PDB strings in driver)

```
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\usb\UsbAudioDevice.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\usb\audstream.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\usb\CStreamFormat.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\usb\isocurb.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\usb\usblib.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\AudioFramework\AsioClient.cpp
D:\a\driver-focusrite-pcie-usb\driver-focusrite-pcie-usb\source\driver\AudioFramework\AudioEngine.cpp
```

---
[← LED Control API](09-led-control-api-discovery.md) | [Index](README.md) | [Driver Binary Analysis →](11-driver-binary-analysis.md)
