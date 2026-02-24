# TRANSACT Protocol Decoded — API Monitor Capture Analysis

## Summary

Analysis of an API Monitor capture of Focusrite Control 2 (FC2) communicating with the Scarlett 2i2 4th Gen revealed the **correct TRANSACT buffer format**. This resolved the primary blocker — all previous prototype attempts used the wrong input format, causing "Incorrect function" errors and 3 BSODs.

The TRANSACT format is **not** a raw Scarlett2 USB packet. It is a simpler Windows-driver-level protocol:

```
[u64 session_token LE] [u32 command LE] [u32 padding=0] [payload...]
```

## Capture Details

| Property | Value |
|----------|-------|
| Tool | API Monitor v2 Alpha-r13 (64-bit) |
| Target | Focusrite Control 2.exe |
| Capture file | `api-monitor-capture.apmx64` (2.3 MB, ZIP-based) |
| Total TRANSACT calls | 282 |
| Unique command codes | 35 |
| Dominant command | `0x00010001` (meter polling, 246 of 282 calls) |

## TRANSACT Input Buffer Format

Every IOCTL `0x222008` input buffer from FC2 follows this structure:

```
Offset  Size   Field           Description
------  ----   -----           -----------
0       8      session_token   u64 LE — 0 for first two calls, then kernel-assigned token
8       4      command         u32 LE — Windows driver command code
12      4      padding         u32 LE — always 0
16+     var    payload         command-specific data
```

**Total minimum size**: 16 bytes (token + cmd + pad, no payload).

This is fundamentally different from what we previously assumed:
- **Wrong** (raw Scarlett2 USB packet): `[cmd:u32][size:u16][seq:u16][error:u32][pad:u32][payload]`
- **Wrong** (SwRoot envelope): `[8-byte hdr][proto:u16][subcmd:u16][payload]`
- **Correct** (from capture): `[token:u64][cmd:u32][pad:u32][payload]`

## Session Initialization Sequence

FC2 performs these steps in order after opening `\pal`:

### Step 1: INIT (IOCTL 0x222000)

Synchronous, no input, 16-byte output. Unchanged from previous understanding.

### Step 2: USB_INIT (IOCTL 0x222008, cmd=0x00010400)

First TRANSACT call. Initializes the USB session. The Linux driver reads an 84-byte payload from this response containing the firmware build number at bytes 8-11.

```
Input (16 bytes):
  token:   0x0000000000000000  (no session yet)
  command: 0x00010400
  padding: 0x00000000

Output (8 bytes):
  SwRoot returns an 8-byte response for USB_INIT.
```

> **Note**: The Linux driver's raw USB INIT_2 (a separate command) returns 84 bytes including the firmware build number at bytes 8-11. SwRoot performs USB_INIT internally (combining INIT_1 + INIT_2) but only returns 8 bytes to the caller. The firmware version is more reliably read from the descriptor header (see below).

### Step 3: GET_CONFIG (IOCTL 0x222008, cmd=0x00040400)

Second TRANSACT call. Reads 96 bytes of device configuration. **The session token is at bytes 8-15 of the response.**

```
Input (16 bytes):
  token:   0x0000000000000000  (still token=0)
  command: 0x00040400
  padding: 0x00000000

Output (96 bytes):
  Byte 0-7:   status/padding (zeros)
  Byte 8-15:  SESSION TOKEN (u64 LE) ← used for ALL subsequent calls
  Byte 16-19: 0x29 = 41 (unknown)
  Byte 20-23: sample rate (u32 LE, e.g. 48000 = 0xBB80)
  Byte 24-27: zeros
  Byte 28-31: 128 (unknown, buffer-related?)
  Byte 32-35: 16 (unknown)
  Byte 36-39: 1024 (unknown)
  Byte 40-43: 2 (may be input count?)
  Byte 44-47: 4 (may be channel count?)
  Byte 48-55: session token (repeated)
  Byte 56-87: zeros
  Byte 88-91: 1
  Byte 92-95: zeros
```

**Confirmed during prototyping**: GET_CONFIG returned `...00 50 F0 AB 85 AE FF FF...` at bytes 8-15, matching FC2's token `0050f0ab85aeffff` exactly.

**Confirmed by Focusmute**: Byte 20 = 48000 (sample rate). Firmware version is NOT in GET_CONFIG — it's in the descriptor header.

### Step 4+: Subsequent Commands

All further TRANSACT calls use the session token from GET_CONFIG bytes 8-15.

### Complete FC2 Initialization Sequence

Decoded from an API Monitor capture of FC2 connecting to a Scarlett 2i2 4th Gen via USB. All 64 TRANSACT calls in order:

```
 #  SwRoot      Raw USB     Name              In Out  Payload
── ────────── ────────── ──────────────── ─── ──── ────────────────────
 1  0x00010400  (combined)  USB_INIT          16    8  —
 2  0x00040400  (internal)  GET_CONFIG        16   96  — (session token at bytes 8-15)
 3  0x00020000  0x00000002  INIT_2            16   96  —
 4  0x00010000  0x00000001  AUTH_STEP         18    9  idx=1
 5  0x00010000  0x00000001  AUTH_STEP         18    9  idx=2
 …  (6 more AUTH_STEP calls with idx=3-8, each returning 1 byte)
12  0x00020000  0x00000002  INIT_2            16   96  —
13  0x00000003  0x00003000  MUX_INFO          16   20  —
14  0x00000002  0x00002000  MIX_INFO          16   16  —
15  0x00000001  0x00001000  METER_INFO        16   12  —
16  0x00000004  0x00004000  INFO_FLASH        16   24  —
17  0x00010004  0x00004001  INFO_SEGMENT      20   32  segment=0
 …  (4 more INFO_SEGMENT calls for segments 1-4)
22  0x00040006  0x00006004  GET_SYNC          16   12  —
23  0x00020006  0x00006002  CLOCK_2           16   16  —
24  0x00050006  0x00006005  CLOCK_5           16   12  —
25  0x000C0800  0x0080000C  INFO_DEVMAP       16   12  — (schema metadata)
26  0x000D0800  0x0080000D  GET_DEVMAP        20 1032  page=0
 …  (5 more GET_DEVMAP pages 1-5; page 5 returns 221 bytes)
32  0x00000800  0x00800000  GET_DESCR         24  728  offset=0, len=720 (full APP_SPACE)
33  0x00010003  0x00003001  GET_MUX           20   56  (table 1, 12 entries)
34  0x00010001  0x00001001  GET_METER         24  272  — (first poll)
35  0x00050004  0x00004005  READ_SEGMENT      28   83  segment=3, offset=0, len=75
36  0x00120401  (internal)  DRIVER_INFO       16  228  — (220 bytes data)
37  0x00010003  0x00003001  GET_MUX           20   56  (table 0, 12 entries)
38  0x00040006  0x00006004  GET_SYNC          16   12  — (repeated)
39  0x00020006  0x00006002  CLOCK_2           16   16  — (repeated)
40  0x00050006  0x00006005  CLOCK_5           16   12  — (repeated)
41+ 0x00010001  0x00001001  GET_METER         24  272  — (24 more polls @ ~22.5 Hz)
```

**Key finding: Zero SET_DESCR. Zero DATA_NOTIFY.** FC2 does not write any descriptor values during initialization — it only reads state. The firmware handles all LED rendering internally.

### FC2 Init Phases

1. **Handshake** (#1-2): USB_INIT + GET_CONFIG to establish session
2. **Authentication** (#3-12): INIT_2 → 8 AUTH_STEP rounds → INIT_2
3. **Topology discovery** (#13-21): Query meter/mix/mux counts, flash segments
4. **Clock/sync** (#22-24): Read sync and clock status
5. **Schema + descriptor** (#25-32): Read firmware API schema, then full APP_SPACE
6. **Routing + config** (#33-40): Read mux tables, flash settings, driver info, re-read clock
7. **Polling** (#41+): Continuous GET_METER at ~22.5 Hz

## Windows Driver Command Codes

These are NOT Scarlett2 USB protocol commands. They are Windows-driver-level commands handled by FocusriteUsbSwRoot.sys, which translates them into USB operations internally.

### SwRoot ↔ Raw USB Mapping Formula

The SwRoot driver command codes map to raw Scarlett2 USB commands using this formula:

```
raw_usb  = ((swroot_cmd & 0xFFFF) << 12) | (swroot_cmd >> 16)
swroot_cmd = ((raw_usb >> 12) & 0xFFFF) | ((raw_usb & 0xFFF) << 16)
```

Think of it as: SwRoot = `[operation:u16][category:u16]`, raw = `[0000][category << 12 | operation]`.

Verified against all 18 known command pairs — no exceptions found.

**Exception:** Commands with suffix `0x0400` or `0x0401` (USB_INIT, GET_CONFIG, DRIVER_INFO) appear to be SwRoot-internal and don't map to single raw USB commands.

### Complete Command Table

| SwRoot | Raw USB | Name | Input | Output | Payload | Status |
|--------|---------|------|-------|--------|---------|--------|
| `0x00010400` | (combined) | **USB_INIT** | 16 | 8 | — | Confirmed |
| `0x00040400` | (internal) | **GET_CONFIG** | 16 | 96 | — | Confirmed |
| `0x00020000` | `0x00000002` | **INIT_2** | 16 | 96 | — | Confirmed |
| `0x00010000` | `0x00000001` | **AUTH_STEP** | 18 | 9 | `[index:u16]` (1-8) | Identified |
| `0x00000001` | `0x00001000` | **METER_INFO** | 16 | 12 | — | Confirmed |
| `0x00000002` | `0x00002000` | **MIX_INFO** | 16 | 16 | — | Confirmed |
| `0x00000003` | `0x00003000` | **MUX_INFO** | 16 | 20 | — | Confirmed |
| `0x00000004` | `0x00004000` | **INFO_FLASH** | 16 | 24 | — | Confirmed |
| `0x00010004` | `0x00004001` | **INFO_SEGMENT** | 20 | 32 | `[segment:u32]` | Confirmed |
| `0x00050004` | `0x00004005` | **READ_SEGMENT** | 28 | varies | `[seg:u32][off:u32][len:u32]` | Confirmed |
| `0x00040006` | `0x00006004` | **GET_SYNC** | 16 | 12 | — | Confirmed |
| `0x00020006` | `0x00006002` | **CLOCK_2** | 16 | 16 | — | Confirmed |
| `0x00050006` | `0x00006005` | **CLOCK_5** | 16 | 12 | — | Confirmed |
| `0x000C0800` | `0x0080000C` | **INFO_DEVMAP** | 16 | 12 | — | Confirmed |
| `0x000D0800` | `0x0080000D` | **GET_DEVMAP** | 20 | 1032 | `[page:u32]` | Confirmed |
| `0x00000800` | `0x00800000` | **GET_DESCR** | 24 | varies | `[offset:u32][size:u32]` | Confirmed |
| `0x00010800` | `0x00800001` | **SET_DESCR** | varies | 8 | `[offset:u32][size:u32][data]` | Confirmed |
| `0x00020800` | `0x00800002` | **DATA_NOTIFY** | 20 | 8 | `[event_id:u32]` | Confirmed |
| `0x00010001` | `0x00001001` | **GET_METER** | 24 | 272 | `[pad:u16][count:u16][magic:u32]` | Confirmed |
| `0x00010003` | `0x00003001` | **GET_MUX** | 20 | 56 | `[pad:u16][count_or_table:u16]` | Confirmed |
| `0x00120401` | (internal) | **DRIVER_INFO** | 16 | 228 | — | Confirmed |

"Confirmed" = tested in our code (via direct use or research probing). "Identified" = command code and payload decoded from FC2 API Monitor capture but not yet tested.

### Command Notes

**INIT_2** (`0x00020000` / raw `0x00000002`): Same as the raw USB INIT_2 step. FC2 sends this twice during init — once before and once after the AUTH_STEP sequence. Returns 88 bytes of data (96 - 8 header). Confirmed response layout:

| Offset | Size | Value (2i2 4th Gen) | Field |
|--------|------|---------------------|-------|
| 0 | 4 | 3 | Protocol version (?) |
| 4 | 4 | 0x0060C006 | Unknown flags |
| 8 | 4 | 2417 | Firmware build number |
| 12 | 4 | 0x00100000 (1 MB) | Flash size |
| 16 | 12 | `Dec  9 2025\0` | Build date string |
| 28 | 4 | `Focu` | Start of build tool string |
| 32 | 8 | `02:12:53` | Build time string |
| 52 | 4 | 0x8219 (33305) | Product ID |
| 72 | 4 | 0x0003186A | Unknown (serial fragment?) |

**AUTH_STEP** (`0x00010000` / raw `0x00000001`): Undocumented in the Linux driver. Sent 8 times with indices 1-8, each returning 1 byte. Appears between two INIT_2 calls. Likely a Gen 4 authentication or device configuration handshake. Not needed for our use case (Focusmute already authenticates successfully without this step).

**METER_INFO** (`0x00000001` / raw `0x00001000`): Returns 4 bytes of topology metadata including meter count (66 for 2i2). Confirmed via `meters` subcommand.

**MIX_INFO** (`0x00000002` / raw `0x00002000`): Returns 8 bytes of mixer topology. Confirmed response: `04 04 10 0D 07 00 00 00` — interpreted as 4 input channels, 4 output channels, 16 mix nodes (4×4 matrix), 13 (total output ports?), 7 (capability flags?).

**MUX_INFO** (`0x00000003` / raw `0x00003000`): Returns 12 bytes of routing mux topology. Confirmed response: `0C 00 00 00 00 00 00 00 01 00 00 00` — 12 mux routing destinations, 1 mux table active (single sample rate band).

**GET_SYNC** (`0x00040006` / raw `0x00006004`): Returns 4 bytes. Confirmed: value `1` = synced (locked to internal clock).

**CLOCK_2 / CLOCK_5** (`0x00006002` / `0x00006005`): Clock/sync commands in the same category as GET_SYNC (`0x00006004`). Appear twice in the init sequence (before and after schema reads). Confirmed: CLOCK_5 returns 4 bytes (current sample rate as u32 LE, e.g. 96000). CLOCK_2 returns 8 bytes (sample rate at offset 0, plus 2 additional u16 values at offset 4-7 — possibly min/max rate indices or clock source flags).

**DRIVER_INFO** (`0x00120401`): Returns 220 bytes of data. SwRoot-internal identity block. Confirmed layout:

| Offset | Size | Value (2i2 4th Gen) | Field |
|--------|------|---------------------|-------|
| 0 | 4 | 0x1235 (4661) | Vendor ID |
| 4 | 4 | 0x8219 (33305) | Product ID |
| 8 | 4 | 3 | Unknown (interface count?) |
| 12 | 4 | 0x12358219 | VID:PID combined |
| 16 | 4 | 2417 | Firmware build number |
| 20 | 64 | `Focusrite` | Manufacturer name (null-padded) |
| 84 | 64 | `Scarlett 2i2 4th Gen` | Product name (null-padded) |
| 148 | 64 | `S2G6HVK563186A` | Serial number (null-padded) |

Useful for multi-device identification without USB descriptor enumeration.

### GET_DESCR Payload Format

```
Offset  Size  Field    Description
------  ----  -----    -----------
0       4     offset   u32 LE — byte offset into descriptor
4       4     size     u32 LE — number of bytes to read
```

Example: Read full 720-byte descriptor:
```
Input: [token:8][0x00000800:4][0:4][offset=0:4][size=720:4] = 24 bytes
Output: 728 bytes (8-byte header + 720 bytes data)
```

### SET_DESCR Payload Format (Inferred)

```
Offset  Size  Field    Description
------  ----  -----    -----------
0       4     offset   u32 LE — byte offset into descriptor
4       4     length   u32 LE — number of bytes to write
8       var   data     bytes to write
```

Example: Set `enableDirectLEDMode = 2` (halos only) at offset 77:
```
Input: [token:8][0x00010800:4][0:4][offset=77:4][length=1:4][0x02:1] = 25 bytes
Output: 8 bytes (acknowledgment)
```

## Output Format

All TRANSACT outputs follow this pattern:

```
Offset  Size  Field    Description
------  ----  -----    -----------
0       8     header   Response header (status/metadata)
8       var   data     Response payload
```

Total output size = 8 + data_length. This was confirmed across all captured calls:
- USB_INIT: 8 bytes (0 data + 8 header, or token IS the response)
- GET_CONFIG: 96 bytes
- GET_DESCR: 8 + requested size (e.g., 8 + 720 = 728)
- GET_METER: 8 + 264 = 272

## Relationship to Previous Protocol Understanding

| Layer | Command Codes | Where |
|-------|--------------|-------|
| **Windows driver** (this doc) | `0x00010400`, `0x00000800`, etc. | DeviceIoControl input buffer |
| **Scarlett2 USB** (see doc 13) | `0x00800000`, `0x00800001`, etc. | USB control transfer payload |

SwRoot.sys translates Windows driver commands into Scarlett2 USB commands internally. Focusmute only needs to speak the Windows driver protocol.

See [13-protocol-reference.md](13-protocol-reference.md) for the complete raw USB protocol, all command codes, SwRoot mapping table, config items, and notification details from the Linux kernel driver.

## Descriptor Schema (Decoded)

The schema is retrieved in two steps:

1. **INFO_DEVMAP** (`cmd=0x000C0800`): Returns `{ u16 unknown, u16 config_len }` after the 8-byte header. `config_len` (u16 LE at response offset 10) is the base64 content length in bytes.
2. **GET_DEVMAP** (`cmd=0x000D0800`): Read `ceil(config_len / 1024)` pages. Each page has an 8-byte header + 1024 bytes payload. Concatenate payloads and truncate to `config_len`.

The concatenated payload is **base64-encoded, zlib-compressed JSON**. Strip trailing null bytes, base64 decode, zlib decompress → ~25KB JSON describing the complete `APP_SPACE` structure.

For the Scarlett 2i2 4th Gen (fw 2.0.2417.0): `config_len=5333`, 6 pages, decompresses to ~25KB.

### Schema Structure

```json
{
  "enums": { /* enum definitions with named values */ },
  "structs": {
    "APP_SPACE": {
      "members": {
        "fieldName": {
          "type": "uint8|uint16|uint32|int8|bool|...",
          "offset": <byte offset in descriptor>,
          "size": <total byte size>,
          "array-shape": [N] or null,
          "notify-device": <FCP message type or null>,
          "set-via-parameter-buffer": true|false
        }
      }
    }
  },
  "device-specification": { /* inputs, outputs, routing, etc. */ }
}
```

### Descriptor Header (Firmware Version)

The first 16 bytes of the descriptor (offset 0) contain version information:

| Offset | Size | Field | Example Value |
|--------|------|-------|---------------|
| 0 | 4 | configSize (u32 LE) | 720 (0x2D0) |
| 4 | 2 | versionMajor (u16 LE) | 2 |
| 6 | 2 | versionMinor (u16 LE) | 0 |
| 8 | 4 | versionStageRelease (u32 LE) | 2417 (0x971) — this is the build number |
| 12 | 4 | versionBuildNr (u32 LE) | 0 (unused on 2i2 Gen 4) |

**Confirmed by Focusmute CLI**: Scarlett 2i2 4th Gen reports firmware **2.0.2417.0** (matching FC2's display exactly). The Linux driver's minimum for this model is build 2115.

The `versionStageRelease` field (offset 8) corresponds to the Linux driver's `firmware_version` integer — the build number printed as `"Firmware version %d\n"`. FC2 formats the version as `major.minor.stageRelease.buildNr`.

### Device Name (offset 16, 32 bytes)

The descriptor contains a null-terminated device name string at offset 16 (32 bytes max):

```
"Scarlett 2i2 4th Gen-XXXXXXXX"
```

The suffix after the dash is a partial serial fragment (last 7-8 hex digits of an internal ID). The full serial number is available from the USB device instance ID via SetupDi enumeration.

### Serial Number

The serial is NOT in the descriptor or GET_CONFIG response. It is stored in the USB device descriptor's `iSerialNumber` string and exposed through the Windows device instance ID:

```
USB\VID_1235&PID_8219\<SERIAL>
```

Focusmute reads this by enumerating USB devices with `VID_1235` (Focusrite) via `SetupDiEnumDeviceInfo` + `SetupDiGetDeviceInstanceIdW` and extracting the third path segment.

### Driver Version

The Windows driver version (`4.143.0.261`) is read from the `FocusriteUsbSwRoot.sys` file's PE version info via PowerShell:

```powershell
(Get-Item 'C:\Windows\System32\drivers\FocusriteUsbSwRoot.sys').VersionInfo.FileVersion
```

### Key Fields for LED Control

| Field | Offset | Type | Size | notify-device | set-via-param-buf |
|-------|--------|------|------|--------------|-------------------|
| `enableDirectLEDMode` | 77 | uint8 | 1 | **null** | false |
| `directLEDChannel` | 78 | uint8[2] | 2 | null | false |
| `directLEDDevice` | 80 | uint8 | 1 | null | false |
| `directLEDColour` | 84 | uint32 | 4 | 8 (SINGLE_LED) | false |
| `directLEDIndex` | 88 | uint8 | 1 | 8 (SINGLE_LED) | false |
| `directLEDValues` | 92 | uint32[40] | 160 | 5 (LED_CONTROL) | false |
| `LEDcolors` | 384 | uint32[11] | 44 | 9 (UPDATE_COLORS) | false |
| `LEDthresholds` | 349 | uint8[25] | 25 | 21 (METER_THRESHOLDS) | false |
| `brightness` | 711 | eBrightnessMode | 1 | 37 (BRIGHTNESS) | **true** |

### LED-Related Enums

```
eDIRECT_LED_MODE: { eDirectLEDModeOff: 0, eDirectLEDModeAll: 1, eDirectLEDModeHalosOnly: 2 }
eBrightnessMode:  { eBrightness_High: 0, eBrightness_Medium: 1, eBrightness_Low: 2 }
```

### FCP Message Types (notify-device values)

```
eMSG_DIRECT_LED_CONTROL: 5   — triggered by directLEDValues write
eMSG_DIRECT_SINGLE_LED:  8   — triggered by directLEDColour/directLEDIndex write
eMSG_UPDATE_COLORS:       9   — triggered by LEDcolors write
eMSG_BRIGHTNESS:         37   — triggered by brightness write
```

### Parameter Buffer Mechanism

Fields with `set-via-parameter-buffer: true` are set by writing to:
- `parameterValue` (offset 252, uint8) — the value to set
- `parameterChannel` (offset 253, uint8) — the channel/index for the parameter being set

This mechanism may be required for certain fields instead of direct SET_DESCR.

### LED Control: SOLVED

**`LEDcolors[]` is the working mechanism.** Writing to descriptor offset 384 (11 x uint32, `notify-device: 9 = eMSG_UPDATE_COLORS`) via SET_DESCR changes all LED halo colors immediately. No `enableDirectLEDMode` change or brightness write needed.

#### Color Format: `0xRRGGBB00`

RGB values are shifted left by 8 bits. The lowest byte is unused (always 0).

```
RED     = 0xFF000000
GREEN   = 0x00FF0000
BLUE    = 0x0000FF00
WHITE   = 0xFFFFFF00  (slight pink tint due to LED hardware)

Formula: (R << 24) | (G << 16) | (B << 8)
```

#### LEDcolors[11] Semantics

The 11 entries are a **metering gradient palette**. At low or no signal levels, only `LEDcolors[0]` is visible (the base/idle color). Higher indices correspond to higher signal levels in the metering display.

#### Example: Set All Halos to Cyan

```
SET_DESCR payload:
  offset = 384 (0x180)    — LEDcolors start
  length = 44  (11 × 4)   — all 11 entries
  data   = [0x00FFFF00] × 11
```

#### Confirmed Colors

All of the following display correctly:
RED (`0xFF000000`), ORANGE (`0xFF800000`), YELLOW (`0xFFFF0000`), GREEN (`0x00FF0000`), CYAN (`0x00FFFF00`), BLUE (`0x0000FF00`), PURPLE (`0x80008000`), MAGENTA (`0xFF00FF00`), WHITE (`0xFFFFFF00`).

#### Restore Original Colors

Write back the original descriptor bytes `[384..428]` (saved before modification) to restore the normal metering gradient.

#### DATA_NOTIFY: The Activation Mechanism

**SET_DESCR alone is not enough.** After writing descriptor fields, you MUST send a **DATA_NOTIFY** command to tell the firmware to act on the new values. Without this, writes persist in the descriptor but the firmware never processes them.

```
DATA_NOTIFY (SwRoot cmd 0x00020800, raw USB 0x00800002):
  Input:  [token:u64][0x00020800:u32][pad=0:u32][event_id:u32]  = 20 bytes
  Output: 8 bytes (acknowledgment)
```

The payload is JUST `[event_id:u32]` (4 bytes) — the `notify-device` value from the schema. This was discovered by studying Geoffrey Bennett's Linux kernel driver (`mixer_scarlett2.c`), which sends a data notification after every descriptor write.

**Correct write sequence:** SET_DESCR(offset, data) → DATA_NOTIFY(event_id)

**Key event IDs:**
| Event ID | Constant | Activates |
|----------|----------|-----------|
| 5 | eMSG_DIRECT_LED_CONTROL | directLEDValues |
| 8 | eMSG_DIRECT_SINGLE_LED | directLEDColour/Index |
| 9 | eMSG_UPDATE_COLORS | LEDcolors |
| 21 | eMSG_METER_THRESHOLDS | LEDthresholds |
| 37 | eMSG_BRIGHTNESS | brightness |

#### LED Control Status

| Field | Offset | Status |
|-------|--------|--------|
| `LEDcolors` | 384 | **WORKS** — SET_DESCR + DATA_NOTIFY(9). All 11 entries same = solid color on all halos. Mixed values = custom metering gradient with per-halo behavior based on signal level. |
| `enableDirectLEDMode` + `directLEDValues` | 77, 92 | **WORKS** — Write mode=2 via SET_DESCR, then write directLEDValues + DATA_NOTIFY(5). Gives solid base color on halos. Metering still overlays on top. |
| `directLEDColour` + `directLEDIndex` | 84, 88 | **WORKS** — SET_DESCR (colour then index) + DATA_NOTIFY(8). Updates only the targeted LED with zero side effects. No mode change needed. Earlier failure was due to incorrect write ordering. |
| `brightness` | 711 | Untested via parameter buffer mechanism. |

#### enableDirectLEDMode + DATA_NOTIFY Details

`enableDirectLEDMode` has `notify-device: null` — it does NOT need its own notification. The firmware checks the mode value when processing DATA_NOTIFY(5) for directLEDValues. Simply write the mode via SET_DESCR, then write directLEDValues and send DATA_NOTIFY(5).

#### directLEDValues Index Mapping (2i2 4th Gen)

Hardware mapping confirmed Input 1 and Input 2 are independently addressable. Testing with mode=2 (halos only) initially masked the independent control.

| Indices | Controls | Notes |
|---------|----------|-------|
| 0 | Input 1 — "1" number indicator | |
| 1-7 | Input 1 — Halo ring (7 segments) | |
| 8 | Input 2 — "2" number indicator | |
| 9-15 | Input 2 — Halo ring (7 segments) | |
| 16-26 | Output — Halo ring (11 segments) | |
| 27 | Select button LED 1 | |
| 28 | Inst button | |
| 29 | 48V button | |
| 30 | Air button | |
| 31 | Auto button | |
| 32 | Safe button | |
| 33-34 | Direct button (2 LEDs) | |
| 35 | Select button LED 2 | |
| 36 | Direct button crossed rings | |
| 37-38 | Output indicator (2 LEDs) | |
| 39 | USB symbol | |

**Input 1 (0-7) and Input 2 (8-15) are independently addressable.** Button LEDs (27-39) are suppressed when `enableDirectLEDMode=2` (halos only).

#### Per-Halo Metering via LEDcolors (Recommended for Mute Indicator)

The metering gradient approach naturally discriminates per-halo based on signal level:
- Set `LEDcolors[0]=0x00000000` (black), `[1-10]=0xFF000000` (red), then DATA_NOTIFY(9)
- Only halos receiving audio signal glow in the mute color
- Idle halos (e.g. Input 2 with no signal) stay dark
- This is the only known method for per-input-halo color control on the 2i2

#### Early Iteration Failures Explained

Early iterations that tested direct LED mode without DATA_NOTIFY produced "non-functional" results because the firmware was never told to process the new descriptor values. Adding DATA_NOTIFY(event_id) after each SET_DESCR write resolves all failures. (Note: directLEDColour+Index via DATA_NOTIFY(8) was initially reported as non-functional, but was later confirmed working — the earlier failure was due to incorrect write ordering; colour must be written before index.)

#### Button LEDs Are Not Directly Color-Controllable via Feature Toggles

API Monitor capture of FC2 toggling the Air button revealed that FC2 simply writes the Air mode value (0, 1, or 2) to descriptor offset 634 via SET_DESCR. The firmware internally maps the mode to a button LED color:

| Air Mode | Value | Button LED Color |
|----------|-------|-----------------|
| Off | 0 | White |
| On | 1 | Green |
| Presence | 2 | Orange |

No color data is sent for button LEDs (Select, 48V, Air, Safe) — they are state indicators tied to feature state. However, some button LEDs ("cache-dependent" ones like Select, Auto, Output, USB) can have their color set indirectly via `directLEDValues` — see `09-led-control-api-discovery.md` for the full button LED category breakdown.

#### API Monitor: FC2 During Physical Button Presses (Select × 4, Inst × 4)

A second API Monitor capture was taken while pressing the physical Select button 4 times, then the Inst button 4 times on the device.

**Select button presses (4 toggles)** — FC2 reads:
| Offset | Field | Size | Count |
|--------|-------|------|-------|
| 334 (0x14E) | `inputChannelsLink` | 2 | 8 (4 request/response pairs) |
| 331 (0x14B) | `selectedInput` | 1 | 8 (4 request/response pairs) |

**Inst button presses (4 toggles)** — FC2 reads:
| Offset | Field | Size | Count |
|--------|-------|------|-------|
| 75 (0x4B) | `preampInputGain` | 2 | 8 (4 request/response pairs) |
| 60 (0x3C) | `instInput` | 2 | 10 (5 request/response pairs) |

**Zero SET_DESCR. Zero DATA_NOTIFY.** FC2 does not write any LED values for physical button presses.

**Conclusion:** The firmware handles ALL LED rendering internally when physical buttons are pressed. FC2 only reads the resulting state changes. This means:
1. There is no host-side protocol to re-engage firmware LED rendering for number LEDs or button LEDs after a DATA_NOTIFY(8) override.
2. The firmware's internal LED color values (e.g., the green for "selected input") are not exposed through any descriptor and cannot be read back.
3. Restoring number LEDs after DATA_NOTIFY(8) override requires writing back an approximated color via another DATA_NOTIFY(8) call (the firmware does not expose number LED colors in the descriptor).

## Why Previous Attempts Failed

| Iteration | What We Sent | Why It Failed |
|-----------|-------------|---------------|
| v1-v3 | Raw Scarlett2 USB packets (`cmd:u32, size:u16, seq:u16, ...`) | Wrong format — driver expects `[token:u64][cmd:u32][pad:u32]` |
| v4 (SwRoot envelope) | `[8-byte hdr][0x0400][subcmd][payload]` | Wrong format — partially recognized (different error code) but still rejected |
| v5 (subcmd scan) | SwRoot envelope with subcmds 1-8 | **BSOD** — driver divided by payload-derived value |
| **Iteration 7** (correct) | `[token:u64][cmd:u32][pad:u32][payload]` | Matches FC2 exactly — works |

## Research Prototype History

> **Note**: The research prototype was never committed to this repository. It was superseded by the Focusmute app.

### Iteration 7: Correct TRANSACT Format

The iteration 7 prototype implements the full initialization sequence:

1. Open `\pal` with `FILE_FLAG_OVERLAPPED`
2. IOCTL 0x222000 (INIT) — synchronous
3. TRANSACT: `token=0, cmd=0x00010400` → USB_INIT
4. TRANSACT: `token=T, cmd=0x00040400` → GET_CONFIG (session token from bytes 8-15)
5. TRANSACT: `token=T, cmd=0x00000800, payload=[0, 720]` → read full descriptor
6. TRANSACT: `token=T, cmd=0x00010800, payload=[77, 1, 2]` → enable direct LED mode (halos)
7. TRANSACT: `token=T, cmd=0x00010800, payload=[92, 160, colors...]` → set 40 LEDs to RED
8. Verify by re-reading descriptor
9. Wait for Enter, then restore `enableDirectLEDMode=0`

### Iteration 15: LEDcolors Discovery (LED CONTROL ACHIEVED)

First successful visual LED change. Wrote to `LEDcolors[]` (offset 384, 11 x u32, notify-device:9) via SET_DESCR. All halos turned RED immediately.

**Bug found**: `stdin().read(&mut [0u8])` only reads 1 byte but Windows Enter sends `\r\n` — caused sequential tests to pair up (every other test was skipped).

### Iteration 16: stdin Fix

Replaced `stdin().read(&mut [0u8])` with `read_line()` to properly consume the full `\r\n` line ending on Windows. Tests now run one at a time as expected.

### Iteration 17: Full Color Cycling (LED CONTROL CONFIRMED)

Cycles through 9 colors (RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, PURPLE, MAGENTA, WHITE) with Enter between each. All colors display correctly. White has a slight pink tint (LED hardware limitation). Restores original metering gradient on exit.

This sequence is implemented in Focusmute's device communication layer (`crates/focusmute-lib/src/device.rs`).

### Iteration 22: Per-Halo LED Test (pre-DATA_NOTIFY)

Tested 6 approaches (Tests A-F) for per-halo LED control: directLEDValues, directLEDColour+Index, mixed LEDcolors gradient, enableDirectLEDMode, parameter buffer. All failed because DATA_NOTIFY was not sent after SET_DESCR writes. The 0x00020800 command returned error code 7 because it was sent with SET_DESCR format instead of just `[event_id:u32]`.

### Iteration 23: DATA_NOTIFY Breakthrough (LED CONTROL FULLY UNLOCKED)

Discovered DATA_NOTIFY from Geoffrey Bennett's Linux kernel driver source. Tested 4 approaches with proper DATA_NOTIFY:

- **Test A** (directLEDValues[0-39]=RED + NOTIFY(5)): Input 1+2 halos turned solid red. Other LEDs went to default colors. Metering still overlays on top.
- **Test B** (directLEDColour+Index + NOTIFY(8)): No visible change at the time — later confirmed working when colour is written before index (write ordering issue).
- **Test C** (LEDcolors gradient [0]=black, [1-10]=red + NOTIFY(9)): Per-halo metering in red — only halos with signal glow red.
- **Test D** (directLEDValues[0-12]=RED, rest=OFF + NOTIFY(5)): Input halos red, all other LEDs dark. Incomplete restore.

Hardware mapping confirmed indices 0-7 = Input 1, 8-15 = Input 2 — independently addressable.

### Mute Indicator

Focusmute monitors system capture device mute state (WASAPI on Windows, PulseAudio on Linux). Uses the metering gradient approach for per-halo discrimination:
- When muted: LEDcolors[0]=black, [1-10]=red + DATA_NOTIFY(9) — active input halos glow red
- When unmuted: original gradient restored + DATA_NOTIFY(9)
- Ctrl+C handler restores original state on exit

### Linux Kernel Driver Research

Study of Geoffrey Bennett's `mixer_scarlett2.c` confirmed:
- DATA_NOTIFY (`scarlett2_usb_activate_boot_config`) is the missing activation step after descriptor writes
- The 2i2 4th Gen has NO input mute capability — only Vocaster devices have `INPUT_MUTE_SWITCH`
- The `.mute=1` flag in the config is a protocol encoding flag (values 0x02/0x03 for temporary muting during hardware changes), not a user-facing mute control

## Capture File Format

The `.apmx64` file is a ZIP archive containing:

```
api-monitor-capture.apmx64 (ZIP)
├── info              — Binary header (tool version, settings)
├── log/
│   ├── monitoring.txt  — Binary call records
│   ├── timestamp.txt   — Timing data
│   ├── 0000000000.blob — Call data blobs (input/output buffers)
│   ├── 0000000001.blob
│   └── ...
```

Call records in `monitoring.txt` are fixed-size binary entries. Data blobs contain the actual input/output buffer contents referenced by offset from the call records.

---
[← Driver Binary Analysis](11-driver-binary-analysis.md) | [Index](README.md) | [Protocol Reference →](13-protocol-reference.md)
