# LED Control API Discovery - Scarlett 2i2 4th Gen

## Summary
Hidden inside the GET_DEVMAP response data (USB command 0x0080000D, initially misidentified as "AUTH_2") is a **zlib-compressed JSON firmware API schema** that reveals the 4th Gen firmware has a COMPLETE LED control API. This was initially missed because:
1. The GET_DEVMAP data appeared to be base64-encoded authentication tokens
2. The base64 decodes to zlib-compressed binary (magic bytes: `78 da`)
3. The decompressed data is a 24,971-byte JSON document describing the entire firmware API

## How We Found It
1. Initial USB capture analysis misidentified the GET_DEVMAP response as "AUTH_2" (authentication exchange)
2. During the double-check phase, we decoded the base64 and discovered it starts with zlib header `78 da`
3. Decompression revealed a JSON firmware schema with 87 descriptor fields, 17 enums, 5 structs
4. Keyword search found: `kMAX_NUMBER_LEDS`, `directLEDColour`, `eDirectLEDModeHalosOnly`, `LEDcolors`, `eMSG_UPDATE_COLORS`

> **Focusmute implementation note**: All mute modes use the single-LED update mechanism (`directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8)) to color only the number indicator LEDs ("1", "2"). Metering halos and all other LEDs are never touched. The gradient (`LEDcolors[]` + DATA_NOTIFY(9)) and bulk (`directLEDValues` + DATA_NOTIFY(5)) approaches documented below were explored during development but are not used — DATA_NOTIFY(8) provides zero-side-effect updates without requiring mode changes. The button LED side effects described in the "Button LED Categories" section below only apply to full direct LED mode usage (e.g., animations), not to Focusmute's single-LED approach.

## LED Control API

### Direct LED Mode
The device supports three LED control modes via `enableDirectLEDMode` (descriptor offset 77):
- `eDirectLEDModeOff (0)`: Normal metering behavior (default)
- `eDirectLEDModeAll (1)`: All 40 LEDs under direct software control
- `eDirectLEDModeHalosOnly (2)`: Only halo LEDs under direct control

### Individual LED Control
Write a color and LED index to control one LED at a time:
- `directLEDColour` (offset 84, uint32): The color value to set
- `directLEDIndex` (offset 88, uint8): Which LED (0-39)
- Writing `directLEDIndex` triggers firmware message `eMSG_DIRECT_SINGLE_LED`

### Bulk LED Control
Write all 40 LED values at once:
- `directLEDValues` (offset 92, uint32[40], 160 bytes): Array of all 40 LED colors
- Writing triggers firmware message `eMSG_DIRECT_LED_CONTROL`

### Metering Color Scheme
Customize the metering gradient colors:
- `LEDcolors` (offset 384, uint32[11]): 11 color values for the metering gradient
- `LEDthresholds` (offset 349, uint8[25]): 25 threshold levels
- Writing LEDcolors triggers `eMSG_UPDATE_COLORS`

### Brightness Control (CONFIRMED WORKING)
- `brightness` (offset 711, eBrightnessMode): High(0), Medium(1), Low(2)
- Uses parameter buffer write mechanism (set-via-parameter-buffer = true)
- Default value: 0 (High)

**Two write methods tested — both produce visible LED brightness changes:**

| Method | Sequence | Descriptor Updated? | Readback Reliable? |
|--------|----------|--------------------|--------------------|
| A — Parameter buffer (FC2-style) | `SET_DESCR(252, [level])` + `SET_DESCR(253, [37])` | No — fire-and-forget | No |
| B — Direct write + DATA_NOTIFY | `SET_DESCR(711, [level])` + `DATA_NOTIFY(37)` | Yes (async) | Lags by one cycle |

- **Method A**: Firmware applies brightness immediately but never writes the new value back to descriptor offset 711. Readback always returns the pre-write value.
- **Method B**: Descriptor is updated, but DATA_NOTIFY triggers an async firmware update. Reading back immediately after the write returns the *previous* value. A short delay before readback would show the correct value.
- **Restore**: Method B (direct write) is more reliable for restore-and-verify. Method A appears to restore visually but cannot be confirmed via readback.
- **Recommendation**: Use Method B for consistency with existing `SET_DESCR` + `DATA_NOTIFY` LED operations. Do not rely on immediate readback for confirmation.

### Vegas Mode (Startup Animation)
- `vegasBuffer` (offset 256, uint32[6]): Animation frame data
- `vegasUpdate` (offset 280, uint32[4]): Animation update data
- `vegasControl` (offset 296, uint8): Animation control (parameter buffer)

## Color Format — SOLVED

**`0xRRGGBB00`** = `(R << 24) | (G << 16) | (B << 8)`. Lowest byte unused (always 0).

```
RED     = 0xFF000000
GREEN   = 0x00FF0000
BLUE    = 0x0000FF00
WHITE   = 0xFFFFFF00  (slight pink tint from LED hardware)
```

Factory default metering gradient at descriptor offset 384 (0x180):
```
LEDcolors[0]  = 0x00000000 (black/off — no signal)
LEDcolors[1]  = 0xB9000000
LEDcolors[2]  = 0xFFDC0000
LEDcolors[3]  = 0x2DFF0000
LEDcolors[4]  = 0xFF960000
LEDcolors[5]  = 0x1A50FF00
LEDcolors[6]  = 0x2DFF0000
LEDcolors[7]  = 0xFFDC0000
LEDcolors[8]  = 0x9BFF8200
LEDcolors[9]  = 0x2DFF0000
LEDcolors[10] = 0x9BFF8200
```

## USB Write Mechanism — SOLVED

> **UPDATE**: The packet format below is the **raw USB** format. Our app uses the **SwRoot TRANSACT** format instead: `[token:u64][cmd:u32][pad:u32][payload]`. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the full protocol.

Using SET_DESCR (raw USB cmd 0x00800001, SwRoot cmd 0x00010800):
```
TRANSACT format: [token:u64][0x00010800:u32][pad=0:u32][offset:u32][length:u32][data:N]
```

**Critical**: After every SET_DESCR, send DATA_NOTIFY with the field's `notify-device` event ID. Without this, descriptor changes are silently ignored by the firmware.

### Working approach: Set all halos to red via LEDcolors (RECOMMENDED)
1. `SET_DESCR(offset=384, len=44, data=[0x00000000, 0xFF000000 × 10])` → write metering gradient
2. `DATA_NOTIFY(event_id=9)` → activate firmware

### Alternative: Set all halos to red via directLEDValues
1. `SET_DESCR(offset=77, len=1, data=[2])` → enable eDirectLEDModeHalosOnly
2. `SET_DESCR(offset=92, len=160, data=[40 × 0xFF000000])` → set all LED colors
3. `DATA_NOTIFY(event_id=5)` → activate firmware

### Individual LED via DATA_NOTIFY(8) — CONFIRMED WORKING
> **Correction**: Earlier testing reported this as "no visible effect". Subsequent
> hardware testing confirmed DATA_NOTIFY(8) **works** on the 2i2 4th Gen — it
> updates only the targeted LED with zero side effects. The earlier failure was
> likely due to incorrect write ordering (colour must be written before index).

1. `SET_DESCR(offset=84, len=4, data=color)` — write `directLEDColour`
2. `SET_DESCR(offset=88, len=1, data=index)` — write `directLEDIndex`
3. `DATA_NOTIFY(event_id=8)` — activate single LED update

**Behavior:**
- Updates ONLY the targeted LED — zero side effects on any other LED
- No mode change needed (works in mode 0, with `enableDirectLEDMode=0`)
- No `directLEDValues` write needed, no DATA_NOTIFY(5) needed
- Write ordering matters: colour must be written before index

**Number LED firmware behavior:** The "1" and "2" number LEDs (indices 0 and 8) are firmware-colored based on `selectedInput` state — green when the input is selected, white/dim when not. After a DATA_NOTIFY(8) override, the firmware does **not** re-assert control. The only restore mechanism is writing back a color via another DATA_NOTIFY(8) call. Best approximated green (for "selected input"): **~0x40FF0000** (R=0x40, G=0xFF, B=0x00) — this is a visual approximation since the firmware does not expose the number LED colors in the descriptor. An API Monitor capture of Focusrite Control 2 confirmed the firmware handles all LED rendering internally — the host software never writes LED colors for physical button presses.

### Button LED Categories (Mode 0)

After using direct LED mode, button LEDs (indices 27-39) fall into two categories when restoring to mode=0:

**Self-coloring LEDs** — firmware writes color directly to LED hardware during parameter buffer feature toggles. The `directLEDValues` descriptor positions for these LEDs are NOT updated; the firmware drives them internally.
| Index | LED | Feature (notify-device) |
|-------|-----|------------------------|
| 28 | Inst | instInput (13) |
| 29 | 48V | enablePhantomPower (11) |
| 30 | Air | inputAir (15) |
| 32 | Safe | clipSafe (14) |
| 33-34 | Direct | directMonitoring (16) |
| 36 | Direct X | directMonitoring (16) |

**Cache-dependent LEDs** — feature toggles mark the LED as "active" but read COLOR from `directLEDValues`. After direct mode, stale animation data remains in these positions. Must write correct default colors before toggling features.
| Index | LED | Feature (notify-device) | Default Color |
|-------|-----|------------------------|---------------|
| 27 | Select 1 | selectedInput (17) | `0x70808800` (white — confirmed firmware value) |
| 31 | Auto | autogainInProgress (10) | `0x70808800` (white — confirmed firmware value) |
| 35 | Select 2 | selectedInput (17) | `0x70808800` (white — confirmed firmware value) |
| 37 | Output 1 | outputMute (28) | `0x70808800` (white — confirmed firmware value) |
| 38 | Output 2 | outputMute (28) | `0x70808800` (white — confirmed firmware value) |
| 39 | USB | (none — always on) | `0x00380000` (green — confirmed firmware value) |

> **Color note**: The cache-dependent button colors above (`0x70808800` white, `0x00380000` green) are confirmed firmware values read directly from the device's `directLEDValues` descriptor — the firmware writes these exact values to the descriptor positions for cache-dependent buttons. Raw `0xFFFFFF00` appears "off-white" (too bright, pink-tinted) on the LED hardware. **Note**: Focusmute uses DATA_NOTIFY(8) single-LED updates targeting only the number indicator LEDs ("1", "2") — it never writes to `directLEDValues`, `LEDcolors[]`, or any button LED positions.

### DATA_NOTIFY(5) Scope in Mode 0

**Important**: `DATA_NOTIFY(5)` in mode=0 applies `directLEDValues` to ALL 40 LEDs, not just buttons. Stale data in halo positions (indices 0-26) will override the metering gradient. Always:
1. Zero ALL `directLEDValues` first
2. Set only the cache-dependent button positions to their firmware default colors
3. Send `DATA_NOTIFY(5)`
4. Then restore metering gradient via `DATA_NOTIFY(9)` — this takes priority over zeroed halo values

### To restore normal metering (full sequence):

> **Simple restore** (only gradient changed, e.g. mute toggle):
> 1. Write back saved LEDcolors bytes at offset 384
> 2. `DATA_NOTIFY(event_id=9)` → reactivate metering gradient

**Full restore** (after direct LED mode — e.g. rainbow animation):

*Phase A — Pre-restore (on old session, before dropping device):*
1. `SET_DESCR(offset=77, len=1, data=[2])` + `DATA_NOTIFY(5)` → transition to mode 2
2. Wait 100ms
3. `SET_DESCR(offset=77, len=1, data=[0])` + `DATA_NOTIFY(5)` → exit to mode 0
4. Wait 100ms, drop device handle

*Phase B — Restore (on new session):*
1. `SET_DESCR(offset=77, len=1, data=[0])` → confirm mode 0
2. Build clean `directLEDValues[40]`: all zeros, then set cache-dependent button positions to firmware default colors
3. `SET_DESCR(offset=92, len=160, data=...)` → write clean directLEDValues
4. `DATA_NOTIFY(event_id=5)` → update firmware LED cache
5. Write back saved LEDcolors at offset 384
6. `DATA_NOTIFY(event_id=9)` → reactivate metering gradient (overrides zeroed halos)
7. Wait 200ms
8. For each feature in parameter buffer: TOGGLE (write opposite value, DATA_NOTIFY, wait 10ms, write original value, DATA_NOTIFY, wait 15ms) — forces firmware to re-engage native LED management for each button
9. Refresh brightness via parameter buffer + `DATA_NOTIFY(37)`

> **Safety**: Never toggle `enablePhantomPower` (notify 11) or `preampInputGain` (notify 12) — toggling 48V can damage condenser microphones, toggling gain causes audio spikes.

### directLEDChannel and directLEDDevice

Schema fields at offsets 78 and 80, both with `notify-device: null`.

Hardware testing confirmed these fields work correctly on the 2i2 — writes succeed and all LED control mechanisms fire as expected. However, varying their values produces **identical LED behavior** on the 2i2 across all three control mechanisms. They likely serve larger interfaces (4i4, Solo, etc.) with multiple LED sections or devices.

**Exhaustive test matrix (2i2 4th Gen):**

| Mechanism | Channel/Device values tested | Result |
|-----------|------------------------------|--------|
| Write without DATA_NOTIFY | `[1,0]`, `[0,1]`, device=1 | Safe, no firmware action |
| DATA_NOTIFY(8) — "1" number LED | `[1,0]`, `[0,1]`, device=1 | Identical to defaults |
| DATA_NOTIFY(8) — Select 1+2 (dual-LED group) | `[1,0]`, `[0,1]`, `[1,1]` | Both LEDs colored as expected, no rerouting |
| DATA_NOTIFY(8) — halo segments (1-7) | `[1,0]`, `[0,1]`, device=1 | Identical to defaults (metering overwrites immediately) |
| DATA_NOTIFY(9) — metering gradient | `[1,0]`, `[0,1]`, device=1 | Gradient applies globally, no per-halo scoping |

## Live Meter Levels — GET_METER (CONFIRMED WORKING)

The firmware exposes live audio signal levels via GET_METER, polled by FC2 at ~22.5 Hz for its metering display. This provides the missing link for inferring halo LED state: meter levels + `LEDthresholds` + `LEDcolors` = reconstructed halo display.

### METER_INFO (SwRoot 0x00000001, raw 0x00001000)

Returns meter topology metadata. No payload required.

Response (4 bytes): `[num_meters:u16][magic:u16]`

Scarlett 2i2 4th Gen returns: `42 00 0C 5A` → `num_meters=66`, `magic=0x5A0C`.

### GET_METER (SwRoot 0x00010001, raw 0x00001001)

Returns live signal levels for all metering points.

```
Request (8 bytes):
  offset 0: u16 LE  pad = 0
  offset 2: u16 LE  num_meters (66 for 2i2)
  offset 4: u32 LE  magic = 1

Response: num_meters x u32 LE values
```

Values are 12-bit: range 0–4095 (0x0FFF), where 4095 = 0 dBFS. dB conversion: `20 × log10(value / 4095.0)`. Confirmed by the Linux kernel driver's ALSA control definition (`min=0, max=4095, step=1` in `scarlett2_meter_ctl_info`) and `alsa-scarlett-gui`'s display code. FC2 polls at ~22.5 Hz (~44ms interval).

### Meter Index Map (Scarlett 2i2 4th Gen)

Confirmed by hardware testing (ambient noise, Direct Monitor toggle, system audio playback):

| Index | Channel | Notes |
|-------|---------|-------|
| 0 | **Analogue Input 1** | Primary input signal level |
| 1 | **Analogue Input 2** | Zero when nothing plugged in |
| 2 | **USB Capture 1** (PCM out to host) | Always mirrors [0] — Input 1 is unconditionally routed to USB recording |
| 3 | **USB Capture 2** (PCM out to host) | Always mirrors [1] |
| 4 | **USB Playback 1** (PCM in from host) | System audio output to device |
| 5 | **USB Playback 2** (PCM in from host) | System audio output to device |
| 6–9 | Unused | S/PDIF, ADAT slots (2i2 has neither) |
| 10 | **Analogue Output 1** | Receives Input 1 via default mixer routing |
| 11 | **Analogue Output 2** | Receives Input 2 via default mixer routing |
| 12–65 | **Internal mixer bus taps** | Stride-7 pattern; active only when Direct Monitor is ON |

**Direct Monitor effect**: With DM ON, indices 11, 18, 25, 32, 39, 46, 53, 60 (stride-7) become active — these are internal mixer bus metering points along the hardware low-latency path. With DM OFF, the signal still reaches Analogue Output via the standard mixer, but fewer internal metering points register activity.

### Reconstructing Halo LED State

The firmware's metering engine performs:
1. Read signal level from GET_METER (indices 0, 1 for inputs; 10, 11 for outputs)
2. Map level through `LEDthresholds[25]` (descriptor offset 349)
3. Pick color from `LEDcolors[11]` gradient (descriptor offset 384)
4. Drive halo LED segments accordingly

With GET_METER + LEDthresholds + LEDcolors, the halo LED display is fully reconstructible from readable data.

### Dangerous Operations

| Operation | Risk | Recovery |
|-----------|------|----------|
| `selectedInput` toggle + DATA_NOTIFY(17) | Device corruption | USB unplug required |
| `enablePhantomPower` toggle (notify 11) | Hardware damage | Can damage condenser microphones |
| `preampInputGain` toggle (notify 12) | Audio spike | May cause loud audio burst |

## IOCTL_NOTIFY — Push-Based Device Event Notifications (CONFIRMED WORKING)

IOCTL code `0x0022200C` provides real-time, push-based notifications for device state changes (button presses, feature toggles). No polling required — the IOCTL pends until the device fires an interrupt, then completes with the event bitmask.

### Protocol

```
IOCTL: 0x0022200C
Input: 0 bytes
Output: 16 bytes (async — returns STATUS_PENDING until device interrupt)
Pattern: pending-IRP — submit, pends in driver, completes on interrupt, re-submit
```

### Response Format (16 bytes)

```
Offset  Size  Field
------  ----  -----
0       4     u32 LE — type/flags (always 0x00000020 for notifications)
4       4     u32 LE — notification bitmask (matches Linux driver table)
8       8     device context (constant per session)
```

**Note**: The bitmask is at bytes 4-7, NOT bytes 0-3.

### Confirmed Notification Bitmasks (Scarlett 2i2 4th Gen)

| Bitmask | Event | Hardware Trigger |
|---------|-------|-----------------|
| 0x44000000 | `input_level \| input_gain` | Inst button ON (switches to Instrument mode + gain recalc) |
| 0x04000000 | `input_level` | Inst button OFF (returns to Line, no gain change) |
| 0x00800000 | `input_air` | Air button toggle |
| 0x01000000 | `direct_monitor` | Direct Monitor button toggle |
| 0x02000000 | `input_select` | Select button (from Linux driver, untested) |
| 0x08000000 | `input_phantom` | 48V button (from Linux driver, untested) |

### Usage Notes

- IOCTL_NOTIFY uses the **same device handle** as TRANSACT but must be sent on a **separate thread** to avoid blocking concurrent command I/O.
- The IOCTL returns immediately on device interrupt — no timeout needed for active devices.
- FC2 uses a dedicated notification listener thread (Thread 26 in API Monitor capture) and re-submits the IOCTL immediately after each notification.
- For Focusmute's `selectedInput` detection: IOCTL_NOTIFY with bit 0x02000000 (`input_select`) replaces polling — instant detection of Select button presses.

## LED State Readback Summary

The descriptor is a **control interface**, not a state readback interface. The following summarizes what CAN and CANNOT be read:

**Readable LED-related state:**
- `directLEDValues[40]` (offset 92) — only cache-dependent buttons (27, 31, 35, 37, 38, 39) have firmware-written values; all others are 0 unless software writes them
- `LEDcolors[11]` (offset 384) — metering gradient palette (configuration, not current display)
- `LEDthresholds[25]` (offset 349) — signal-to-gradient mapping thresholds
- `brightness` (offset 711) — global brightness: 0=High, 1=Medium, 2=Low
- `selectedInput` (offset 331) — which input is selected (0=Input 1, 1=Input 2); updates on button press, pollable at 100ms
- Feature state flags (`instInput`, `inputAir`, `enablePhantomPower`, `clipSafe`, `directMonitoring`, `autogainInProgress`) — infer which button LEDs are active
- `GET_METER` levels — live 12-bit signal levels per audio channel (see above)

**NOT readable:**
- Current meter levels per LED segment (firmware-internal; must reconstruct from GET_METER + thresholds + gradient)
- Current LED display colors (firmware renders directly to hardware without updating any descriptor field)
- Self-coloring button LED colors (Inst/28, 48V/29, Air/30, Safe/32, Direct/33-34,36 — firmware drives these directly)
- Number LED colors (indices 0, 8 — firmware-colored based on `selectedInput`, color not exposed in descriptor)

## Complete APP_SPACE Layout (87 fields, 720 bytes)
Key fields for our use case:
| Field | Type | Offset | Size | Description |
|-------|------|--------|------|-------------|
| enableDirectLEDMode | uint8 | 77 | 1 | LED control mode (0=off, 1=all, 2=halos) |
| directLEDChannel | uint8[2] | 78 | 2 | Channel selection (works on 2i2 but all values produce identical behavior — likely for multi-section interfaces) |
| directLEDDevice | uint8 | 80 | 1 | Device selection (works on 2i2 but all values produce identical behavior — likely for multi-device interfaces) |
| directLEDColour | uint32 | 84 | 4 | Single LED color |
| directLEDIndex | uint8 | 88 | 1 | LED index (triggers update) |
| directLEDValues | uint32[40] | 92 | 160 | All 40 LED colors |
| outputMute | uint8[2] | 54 | 2 | Output mute (notify=28) |
| preampInputGain | uint8[2] | 75 | 2 | Input gain levels |
| LEDthresholds | uint8[25] | 349 | 25 | Meter threshold levels |
| LEDcolors | uint32[11] | 384 | 44 | Meter gradient colors |
| brightness | eBrightnessMode | 711 | 1 | LED brightness |
| parameterValue | uint8 | 252 | 1 | Param buffer value (0xFC) |
| parameterChannel | uint8 | 253 | 1 | Param buffer channel (0xFD) |

## Access Challenge — SOLVED

> **UPDATE**: Access through the Windows driver is fully working. See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for the complete protocol.

FocusriteUsbSwRoot.sys exposes device interfaces via GUID `{AC4D0455-50D7-4498-B3CD-9A41D130B759}`. Open the `\pal` path with `CreateFileW()`, then use `DeviceIoControl()` with IOCTL `0x00222008` (TRANSACT). Format: `[token:u64][cmd:u32][pad:u32][payload]`.

**Critical**: After SET_DESCR writes, send DATA_NOTIFY (`cmd=0x00020800`, payload=`[event_id:u32]`) to activate the firmware. Without this, descriptor changes are silently ignored.

## Firmware Message Types (eDEV_FCP_USER_MESSAGE_TYPE)
LED-related firmware messages (subset of 40 total `eDEV_FCP_USER_MESSAGE_TYPE` entries):
| ID | Name | Description |
|----|------|-------------|
| 0 | eNO_MESSAGE | No message |
| 5 | eMSG_DIRECT_LED_CONTROL | Direct LED control (bulk) |
| 8 | eMSG_DIRECT_SINGLE_LED | Single LED update |
| 9 | eMSG_UPDATE_COLORS | Update metering colors |
| 21 | eMSG_METER_THRESHOLDS | Update meter thresholds |
| 37 | eMSG_BRIGHTNESS | Set brightness |
| 31-33 | eMSG_VEGAS_* | Vegas animation control |

## Full Schema
The complete 24,971-byte firmware schema JSON is at `device_firmware_schema.json` in this documentation directory.

## Addendum: Unexplored APP_SPACE Descriptor Fields

> Source: cross-reference of our firmware schema against [alsa-scarlett-gui](https://github.com/geoffreybennett/alsa-scarlett-gui) demo configs and recent commits (Feb 2026).

The 2i2 Gen 4 APP_SPACE descriptor has 87 fields across 720 bytes. We documented and tested the LED-related fields extensively. The following fields are **readable via GET_DESCR** but have not been explored on hardware.

### Autogain System

| Field | Offset | Type | Notify | Notes |
|-------|--------|------|--------|-------|
| meanTargetNegDBFS | 305 | u8 | 29 (param-buf) | Autogain mean level target. Default: 18 (= -18 dBFS) |
| peakTargetNegDBFS | 306 | u8 | 30 (param-buf) | Autogain peak level target. Default: 12 (= -12 dBFS) |
| autogainUsePeak | 307 | u8 | 27 (param-buf) | Use peak (1) vs mean (0) for autogain |
| autogainSetMaxGain | 308 | u8 | 26 (param-buf) | Maximum gain limit |
| autogainInProgress | 309 | u8[2] | 10 (param-buf) | Per-channel autogain active flag |
| autogainExitStatus | 311 | u8[2] | 10 | Result code per channel (see `AutogainResult` enum) |
| autogainStats | 313 | int8[2,6] | — | Debug statistics per channel (6 values each) |
| autogainVersion | 325 | u8[2] | — | Algorithm version per channel |

Exit codes: 0=Success, 1=SuccessDRover, 2=WarnMinGainLimit, 3=FailDRunder, 4=FailMaxGainLimit, 5=Clipped, 6=Cancelled.

### Air Plus DSP

| Field | Offset | Type | Notify | Notes |
|-------|--------|------|--------|-------|
| airPlusParams | 528 | appSpaceAirPlusParams | 34 | 104-byte struct: distortionVolume (u32), levelCompensation (u32), bandpassCoeffs (u32[24]) |
| airPlusUsesAir | 632 | u8[2] | 35 (param-buf) | Per-channel Air Plus mode enable |
| previouslySelectedAirMode | 634 | u8[2] | — | Saved Air mode per channel |

Air mode enum: Off=0, Air=1, AirPlus=2. The `bandpassCoeffs[24]` (96 bytes) contains the DSP biquad filter coefficients that define the "Air" EQ curve. Hardware format is `[b0, b1, b2, -a1, -a2] × 2^28` (28-bit fixed-point, a1/a2 negated).

### Clip Safe DSP

| Field | Offset | Type | Notify | Notes |
|-------|--------|------|--------|-------|
| clipSafeParams | 512 | appSpaceClipSafeParams | 22 | 12-byte struct: digitalGainMaxdB (u8), threshdBFS (u8), analogSettlems (u8), drcDelayMicros (u32), compressorRelease (u32) |
| clipSafedroppeddB | 524 | u8[2] | — | How much gain clip safe dropped per channel |
| clipSafeVersion | 526 | u8[2] | — | Algorithm version per channel |

### Front Panel Sleep

| Field | Offset | Type | Notify | Notes |
|-------|--------|------|--------|-------|
| frontPanelSleep | 712 | bool | 38 (param-buf) | Enable front panel sleep mode |
| frontPanelSleepTime | 716 | u32 | 39 | Sleep timeout in seconds |

Exposed in FC2 as **"Automatically turn off all LEDs"** (Device Settings tab). Also triggered by **holding Inst button for 2 seconds** on the hardware. Internal name in SwRoot binary: `ledSleepEnabled` (doc 11). FC2 action: `SetLedSleep` (doc 05).

**Defaults**: Disabled (false). Timeout: 600 seconds (10 minutes).

**Behavior**: After the timeout with no front-panel interaction and no audio passing through the device, the firmware turns off ALL LEDs. This is a complete shutoff, not dimming (distinct from `brightness` at offset 711). The `currentFrontPanelState` (offset 83) tracks the firmware's state machine and changes when sleep activates.

**Wake-up triggers**: Audio passing through the device (input or output), any front-panel control touched (knob, button).

**Interaction with Focusmute's LED control**: When sleep is active, the firmware suppresses all LED rendering. SET_DESCR writes to `directLEDValues` or `LEDcolors` still succeed (descriptor memory is written), but the firmware's rendering pipeline ignores them. Whether DATA_NOTIFY(5/9) wakes the panel is **unconfirmed** — it is a firmware interaction which may or may not count as "activity". Not implemented in the Linux kernel driver or alsa-scarlett-gui.

**Mitigation for Focusmute**: Since the default is disabled, most users are unaffected. For users who enabled sleep in FC2, options include: (a) read offset 712 on startup and warn if enabled, (b) disable sleep on startup and restore on exit, (c) periodically send DATA_NOTIFY to keep the panel awake. Option (a) is recommended as the lowest-risk approach.

### Hardware State (Read-Only)

| Field | Offset | Type | Notes |
|-------|--------|------|-------|
| endlessPotAdcValue | 336 | u32[2] | Raw ADC values from the two volume encoders |
| inputTRSPresent | 345 | u8[2] | **TRS-only** jack detection (1 = plugged in). Does NOT detect XLR — combo jacks have separate sensing for TRS ring/sleeve contact vs XLR latch; only TRS is exposed to firmware. Notification: IOCTL_NOTIFY bit 0x20000000 (`FCP_NOTIFY_TRS_INPUT_CHANGE`). **Hardware-confirmed on 2i2.** |
| usb2Connected | 344 | u8 | USB 2.0 (1) vs USB 3.0 (0) connection |
| inLowVoltageState | 347 | u8 | Low USB bus power detected |
| lowVoltagePinValue | 348 | u8 | Raw low-voltage sensor reading |
| totalSecondsCounter | 376 | u32 | Cumulative device runtime in seconds |
| powerCycleCounter | 380 | u32 | Total power cycle count |
| currentFrontPanelState | 83 | u8 | Current front panel state machine position |
| EPFifoFull | 301 | u8 | Endpoint FIFO overflow flag |
| FCPFifoFull | 302 | u8 | FCP FIFO overflow flag |

### Mixer State

| Field | Offset | Type | Notes |
|-------|--------|------|-------|
| mixerState | 636 | appSpaceMixerChannelParams[2,4] | Per-channel: levelDb (int8), pan (int8), mute (bool), solo (bool), effectiveMute (bool) |
| monoDirectMonitorMixCoeffs | 676 | u16[2,4] | Mono direct monitor mix coefficients |
| stereoDirectMonitorMixCoeffs | 692 | u16[2,4] | Stereo direct monitor mix coefficients |
| directMonitorMainOffset | 81 | u8 | DM main mix offset (notify=6) |
| directMonitorAltOffset | 82 | u8 | DM alt mix offset |
| previouslySelectedDirectMonitorMode | 708 | u8 | Saved DM mode |
| loopbackMirrorsDirectMonitorMix | 709 | bool | Loopback mirrors DM setting |

### Other

| Field | Offset | Type | Notes |
|-------|--------|------|-------|
| inputMutes | 303 | u8[2] | Input mutes (but 2i2 has no INPUT_MUTE_SWITCH — see finding 34) |
| digitalMute | 332 | u8[2] | Digital channel mutes |
| inputChannelsLink | 334 | u8[2] | Stereo link (notify=18, param-buf) |
| meterBallistics | 297 | u8[4] | Meter decay/attack rates (notify=25) |
| meteringEnabled | 329 | u8 | Metering enable (notify=19, param-buf) |
| ffEdition | 710 | bool | Unknown (firmware flag?) |
| logListenId | 444 | u8 | Debug log channel (notify=24, param-buf) |
| logFlags | 445 | u8 | Debug log flags |
| logValues1/2 | 448/480 | u32[8] | Debug log data (32 bytes each) |

### Hardware-Confirmed Values (2i2 4th Gen)

Read via `investigate` command on a production 2i2 (firmware 2.0.2417.0):

| Field | Value | Notes |
|-------|-------|-------|
| inputTRSPresent | [0, 0] with XLR mic plugged in | Confirms TRS-only detection — XLR not sensed |
| totalSecondsCounter | 2,250,322 (26d 1h 5m) | Cumulative, persists across power cycles |
| powerCycleCounter | 245 | Lifetime boot count |
| frontPanelSleep | 0 (disabled) | Confirms factory default |
| frontPanelSleepTime | 600 (10m 0s) | Confirms factory default |
| usb2Connected | 0 | Device on USB 3.x port |
| inputMutes | [0, 0] | No firmware-level mute (as expected — 2i2 has no INPUT_MUTE_SWITCH) |

### Brightness Control (Hardware-Confirmed)

The `brightness` field (offset 711, `eBrightnessMode`) controls LED brightness: 0=High, 1=Medium, 2=Low.

**Working method**: Direct write + DATA_NOTIFY(37):
```
SET_DESCR(offset=711, data=[level])   → write desired brightness
DATA_NOTIFY(37)                        → activate
```
Visual brightness change is **immediate**. Hardware-confirmed on 2i2.

**Non-working method**: Parameter buffer (writing to `parameterValue` at 252, then `parameterChannel` at 253 with value 37). Despite the schema marking brightness as `set-via-parameter-buffer: true`, the parameter buffer mechanism does NOT change brightness. The descriptor readback stays at the original value and no visual change occurs.

**Readback quirk**: After a successful direct write + DATA_NOTIFY, reading back offset 711 immediately returns a **stale value** (the previous brightness, not the just-written value). The firmware appears to process DATA_NOTIFY asynchronously and updates the descriptor readback on a delayed cycle. For practical purposes: write-and-forget is fine; do not rely on immediate readback for confirmation.

---
[← OCA Probing Results](08-oca-probing-results.md) | [Index](README.md) | [USB Access Investigation →](10-usb-access-investigation.md)
