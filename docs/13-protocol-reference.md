# Protocol Reference — Scarlett 2i2 4th Gen

## Summary

Protocol details extracted from Geoffrey Bennett's `mixer_scarlett2.c` in the Linux kernel (`sound/usb/mixer_scarlett2.c`, ~9400 lines). This driver supports Gen 2/3/4 Scarlett, Clarett, and Vocaster devices.

**Critical note:** The Linux driver does NOT implement any LED/brightness/color controls. The LED features (directLEDMode, directLEDValues, LEDcolors, brightness) exist only in the Gen 4 device-map JSON schema and have been implemented independently in Focusmute.

## Raw USB Packet Format

At the raw USB level (not SwRoot), every command uses this 16-byte header:

```
struct scarlett2_usb_packet {
    __le32 cmd;    // offset 0:  command ID
    __le16 size;   // offset 4:  payload size in bytes
    __le16 seq;    // offset 6:  sequence number
    __le32 error;  // offset 8:  error code (0 = success)
    __le32 pad;    // offset 12: padding (must be 0)
    u8     data[]; // offset 16: variable-length payload
};
```

### USB Control Transfer Parameters

Requests use control endpoint 0 on the vendor-specific interface (`bInterfaceClass == 255`):

**TX (sending a request):**
- `bRequest = 2` (SCARLETT2_USB_CMD_REQ)
- `bmRequestType = USB_RECIP_INTERFACE | USB_TYPE_CLASS | USB_DIR_OUT`
- `wValue = 0`, `wIndex = bInterfaceNumber`

**RX (receiving a response):**
- `bRequest = 3` (SCARLETT2_USB_CMD_RESP), or `0` for init step 0
- `bmRequestType = USB_RECIP_INTERFACE | USB_TYPE_CLASS | USB_DIR_IN`
- `wValue = 0`, `wIndex = bInterfaceNumber`

### Command Flow

1. **TX**: Send request via USB control transfer (bRequest=2)
2. **Wait for ACK**: Interrupt notification with bitmask bit 0x00000001 (typically within 1000ms)
3. **RX**: Read response via USB control transfer (bRequest=3)

TX retries on EPROTO: up to 5 retries with exponential backoff (5ms, 10ms, 20ms, 40ms, 80ms). ESHUTDOWN and EPROTO after a REBOOT command are expected (device disconnects).

### Sequence Counter

- `u16`, starts at 0, set to 1 before init steps
- Increments after each command
- Special: when req.seq == 1, resp.seq == 0 is allowed (init exception)
- Timeout: 1000ms per command
- Retries: up to 5 on `-EPROTO` errors, exponential backoff (5, 10, 20, 40, 80ms)

## Complete Raw USB Command Table

| Raw Command | Name | Payload (TX) | Response | Description | Source |
|-------------|------|-------------|----------|-------------|--------|
| `0x00000000` | INIT_1 | none | none | Init step 1, seq forced to 1 | Linux driver |
| `0x00000001` | AUTH_STEP | 2 bytes (index) | 1 byte | Gen 4 auth handshake step (×8, indices 1-8) | FC2 capture |
| `0x00000002` | INIT_2 | none | 84 bytes | Init step 2, firmware version at bytes 8-11 | Linux driver |
| `0x00000003` | REBOOT | none | none | Reboot device | Linux driver |
| `0x00001000` | METER_INFO | none | 4 bytes | Meter topology/configuration | FC2 capture |
| `0x00001001` | GET_METER | 8 bytes | N x u32 | Read meter levels | Linux driver |
| `0x00002000` | MIX_INFO | none | 8 bytes | Mixer topology/configuration | FC2 capture |
| `0x00002001` | GET_MIX | varies | N x u32 | Read mixer gains | Linux driver |
| `0x00002002` | SET_MIX | varies | none | Write mixer gains | Linux driver |
| `0x00003000` | MUX_INFO | none | 12 bytes | Routing mux topology/configuration | FC2 capture |
| `0x00003001` | GET_MUX | 4 bytes | N x u32 | Read routing mux | Linux driver |
| `0x00003002` | SET_MUX | varies | none | Write routing mux | Linux driver |
| `0x00004000` | INFO_FLASH | none | 16 bytes | Flash info (size, segment count) | Linux driver |
| `0x00004001` | INFO_SEGMENT | 4 bytes | 24 bytes | Flash segment info (size, flags, name) | Linux driver |
| `0x00004002` | ERASE_SEGMENT | 8 bytes | none | Erase flash segment | Linux driver |
| `0x00004003` | GET_ERASE | 8 bytes | 1 byte | Erase progress (0xFF = complete) | Linux driver |
| `0x00004004` | WRITE_SEGMENT | 12 + data | none | Write flash segment (max 1012 bytes) | Linux driver |
| `0x00004005` | READ_SEGMENT | 12 bytes | data | Read flash segment (max 1024 bytes) | Linux driver |
| `0x00006002` | CLOCK_2 | none | 8 bytes | Clock/sync info (undocumented) | FC2 capture |
| `0x00006004` | GET_SYNC | none | varies | Clock sync status | Linux driver |
| `0x00006005` | CLOCK_5 | none | 4 bytes | Clock/sync info (undocumented) | FC2 capture |
| `0x00800000` | **GET_DATA** | 8 bytes | data | Read descriptor memory | Linux driver |
| `0x00800001` | **SET_DATA** | 8 + data | none | Write descriptor memory | Linux driver |
| `0x00800002` | **DATA_CMD** | 4 bytes | none | Activate/notify after write | Linux driver |
| `0x0080000c` | INFO_DEVMAP | none | 4 bytes | Schema size info | Linux driver |
| `0x0080000d` | GET_DEVMAP | 4 bytes | 1024 bytes | Schema data block | Linux driver |

Commands marked "FC2 capture" were discovered by analyzing Focusrite Control 2's USB traffic via API Monitor. The `xxx0` variants (METER_INFO, MIX_INFO, MUX_INFO) are category info/count queries — the Linux driver doesn't implement these, using hardcoded per-device topology tables instead.

## SwRoot Command Mapping

On Windows, FocusriteUsbSwRoot.sys wraps raw USB commands into its own IOCTL format with a session token.

### Mapping Formula

```
raw_usb    = ((swroot_cmd & 0xFFFF) << 12) | (swroot_cmd >> 16)
swroot_cmd = ((raw_usb >> 12) & 0xFFFF) | ((raw_usb & 0xFFF) << 16)
```

SwRoot decomposes as `[operation:u16][category:u16]`. Raw USB is `[0000][(category << 12) | operation]`.

**Exception:** Commands with suffix `0x0400` / `0x0401` are SwRoot-internal (USB_INIT, GET_CONFIG, DRIVER_INFO) and don't map to single raw USB commands.

### Complete Mapping Table

| SwRoot | Raw USB | Name | Status |
|--------|---------|------|--------|
| `0x00010400` | INIT_1 + INIT_2 | USB_INIT | Confirmed (v8) |
| `0x00040400` | (internal) | GET_CONFIG | Confirmed (v8) |
| `0x00020000` | `0x00000002` INIT_2 | INIT_2 | Confirmed |
| `0x00010000` | `0x00000001` | AUTH_STEP (×8, undocumented) | FC2 capture |
| `0x00000001` | `0x00001000` | METER_INFO | Confirmed (meters) |
| `0x00000002` | `0x00002000` | MIX_INFO | Confirmed |
| `0x00000003` | `0x00003000` | MUX_INFO | Confirmed |
| `0x00000004` | `0x00004000` INFO_FLASH | INFO_FLASH | Confirmed |
| `0x00010004` | `0x00004001` INFO_SEGMENT | INFO_SEGMENT (×5) | Confirmed |
| `0x00050004` | `0x00004005` READ_SEGMENT | READ_SEGMENT | Confirmed |
| `0x00040006` | `0x00006004` GET_SYNC | GET_SYNC | Confirmed |
| `0x00020006` | `0x00006002` | CLOCK_2 | Confirmed |
| `0x00050006` | `0x00006005` | CLOCK_5 | Confirmed |
| `0x000C0800` | `0x0080000C` INFO_DEVMAP | INFO_DEVMAP | Confirmed (v9) |
| `0x000D0800` | `0x0080000D` GET_DEVMAP | GET_DEVMAP | Confirmed (v9) |
| `0x00000800` | `0x00800000` GET_DATA | GET_DESCR | Confirmed (v8) |
| `0x00010800` | `0x00800001` SET_DATA | SET_DESCR | Confirmed (v8) |
| `0x00020800` | `0x00800002` DATA_CMD | DATA_NOTIFY | Confirmed (v23) |
| `0x00010001` | `0x00001001` GET_METER | GET_METER | Confirmed (meters) |
| `0x00010003` | `0x00003001` GET_MUX | GET_MUX | Confirmed |
| `0x00120401` | (internal) | DRIVER_INFO (220 bytes) | Confirmed |

See [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md) for detailed command notes and FC2 init sequence.

## Initialization Sequence (Raw USB)

From `scarlett2_usb_init()`:

**Step 0** — Read 24 bytes using bRequest=0:
> "cargo cult proprietary initialisation sequence" (driver's own words)

**Set up interrupt URB** for notifications on vendor-specific endpoint.

**Sleep 20ms** to let pending ACKs arrive.

**Step 1** — INIT_1 (`cmd=0x00000000`, seq=1):
```
TX: cmd=0x00000000, size=0, seq=1
RX: cmd=0x00000000, size=0, seq=0
```

**Step 2** — INIT_2 (`cmd=0x00000002`, seq=1):
```
TX: cmd=0x00000002, size=0, seq=1
RX: cmd=0x00000002, size=84, seq=0
```
Firmware version at response bytes 8-11 (le32).

## GET_DATA / SET_DATA / DATA_CMD Detail

### GET_DATA (0x00800000)
```
Request payload (8 bytes):
  offset 0: u32 LE  descriptor_offset
  offset 4: u32 LE  byte_count
Response: raw data of byte_count bytes
```

### SET_DATA (0x00800001)
```
Request payload for single value:
  offset 0: u32 LE  descriptor_offset
  offset 4: u32 LE  size (1, 2, or 4)
  offset 8: u32 LE  value (only first `size` bytes sent on wire!)

Request payload for bulk write:
  offset 0: u32 LE  descriptor_offset
  offset 4: u32 LE  total_byte_count
  offset 8: u8[]    data
```

**Important:** For single values, the USB request size is `8 + size`, not `8 + 4`. Only the significant bytes of the value are transmitted.

### DATA_CMD / Activate (0x00800002)
```
Request payload (4 bytes):
  offset 0: u32 LE  activate_value
Response: none
```

Special activate value `6` = **NVRAM save** (persist config to flash).

## DATA_NOTIFY Event IDs (Complete — 2i2 Gen 4)

All 40 event IDs from the firmware schema (`eDEV_FCP_USER_MESSAGE_TYPE`). Send via DATA_CMD (raw `0x00800002`, SwRoot `0x00020800`) with `[event_id:u32]` payload after SET_DESCR writes.

| ID | Enum Name | Schema Field | Notes |
|----|-----------|-------------|-------|
| 0 | eNO_MESSAGE | — | No-op |
| 1 | eMSG_VOLUME | outputVol | Output volume |
| 2 | eMSG_SWITCH | — | Generic switch |
| 3 | eMSG_SWITCH_CTRL | — | Switch control |
| 4 | eMSG_FLASH_CTRL | enableMSD | MSD (mass storage) switch |
| **5** | **eMSG_DIRECT_LED_CONTROL** | **directLEDValues[40]** | **Bulk LED update** |
| 6 | eMSG_MONITORING_CONTROL | directMonitorMainOffset | Config save (NVRAM) / DM offset |
| 7 | eMSG_FACTORY_TEST_MODE | factoryTestMode | Factory test (param-buf) |
| **8** | **eMSG_DIRECT_SINGLE_LED** | **directLEDColour + directLEDIndex** | **Single LED update** |
| **9** | **eMSG_UPDATE_COLORS** | **LEDcolors[11]** | **Metering gradient update** |
| 10 | eMSG_AUTOGAIN | autogainInProgress[2] | Start/stop autogain (param-buf) |
| 11 | eMSG_PHANTOM_POWER | enablePhantomPower | 48V phantom (param-buf). **DANGER: toggling can damage condenser mics** |
| 12 | eMSG_INPUT_GAIN | preampInputGain[2] | Preamp gain (param-buf). **DANGER: toggling causes audio spikes** |
| 13 | eMSG_INPUT_INST | instInput[2] | Instrument/line level (param-buf) |
| 14 | eMSG_CLIP_SAFE | clipSafe[2] | Clip safe enable (param-buf) |
| 15 | eMSG_INPUT_AIR | inputAir[2] | Air mode (param-buf) |
| 16 | eMSG_DIRECT_MONITORING | directMonitoring | Direct monitor (param-buf) |
| 17 | eMSG_SELECT_CHANNEL | selectedInput | Input select (param-buf). **WARNING: write crashes device** |
| 18 | eMSG_LINK_CHANNEL | inputChannelsLink[2] | Stereo link (param-buf) |
| 19 | eMSG_METERING | meteringEnabled | Metering enable (param-buf) |
| 20 | eMSG_RESET | — | Device reset |
| 21 | eMSG_METER_THRESHOLDS | LEDthresholds[25] | Meter-to-LED threshold mapping |
| 22 | eMSG_SET_CLIPSAFE_CTRL | clipSafeParams | Clip safe DSP parameters |
| 23 | eMSG_PREAMP_ZC_DETECT | — | Preamp zero-crossing detection |
| 24 | eMSG_LOG_LISTEN_ID | logListenId | Debug log channel (param-buf) |
| 25 | eMSG_METER_BALLISTICS | meterBallistics[4] | Meter decay/attack rates |
| 26 | eMSG_AUTOGAIN_MAX_GAIN | autogainSetMaxGain | AG max gain limit (param-buf) |
| 27 | eMSG_AUTOGAIN_PEAKS | autogainUsePeak | AG peak mode (param-buf) |
| 28 | eMSG_DAC_MUTE | outputMute[2] | DAC mute (param-buf) |
| 29 | eMSG_AG_MEAN_TARGET | meanTargetNegDBFS | AG mean target dBFS (param-buf) |
| 30 | eMSG_AG_PEAK_TARGET | peakTargetNegDBFS | AG peak target dBFS (param-buf) |
| **31** | **eMSG_VEGAS_BUF** | **vegasBuffer[6]** | **Vegas animation buffer** |
| **32** | **eMSG_VEGAS_UPD** | **vegasUpdate[4]** | **Vegas animation update** |
| **33** | **eMSG_VEGAS_CTRL** | **vegasControl** | **Vegas animation control (param-buf)** |
| 34 | eMSG_SET_AIRPLUS_PARAM | airPlusParams | Air Plus DSP coefficients (104 bytes) |
| 35 | eMSG_AIR_PLUS_USES_AIR | airPlusUsesAir[2] | Air Plus mode enable (param-buf) |
| 36 | eMSG_FLASH_CTRL_DEFERRED | — | Deferred flash write |
| **37** | **eMSG_BRIGHTNESS** | **brightness** | **LED brightness (param-buf)** |
| 38 | eMSG_FP_SLEEP | frontPanelSleep | Front panel sleep (param-buf) |
| 39 | eMSG_FP_SLEEP_TIME | frontPanelSleepTime | Sleep timeout (u32 seconds) |

**Bold** = LED-related. Events 0-4, 10-18, 29-30, 36 are implemented in the Linux kernel driver. The rest (LED, Vegas, Air Plus, sleep) are schema-only.

## Parameter Buffer Mechanism

Gen 4 devices use a "parameter buffer" for certain config items instead of direct SET_DATA writes.

**Buffer addresses by device:**
| Device | Address |
|--------|---------|
| Solo Gen 4 | 0xD8 |
| **2i2 Gen 4** | **0xFC** |
| 4i4 Gen 4 | 0x130 |
| Vocaster One/Two | 0x1BC |

**Write sequence** (for config items with `pbuf=1`):
1. SET_DATA at `param_buf_addr + 1` (0xFD), size=1, value=**channel/index**
2. SET_DATA at `param_buf_addr` (0xFC), size=1, value=**new value**
3. DATA_CMD with activate value

This matches our schema: `parameterChannel` at offset 253 (0xFD), `parameterValue` at offset 252 (0xFC).

**Mute flag**: Some config items (phantom power, level switch) use a `mute` flag in the kernel driver. When `mute=1`, the driver writes values 0x02/0x03 instead of 0x01/0x00 — the firmware temporarily mutes the audio path during the switch to prevent pops/clicks, then unmutes automatically.

> **Note**: The Linux kernel driver writes channel (0xFD) first, then value (0xFC) — matching the sequence above. Focusmute's brightness test writes in the opposite order (value first, then channel) and both orderings work on hardware. The firmware likely triggers on whichever write comes second.

**NVRAM save note:** Devices with param_buf_addr do NOT need a separate delayed NVRAM save. Devices without it schedule DATA_CMD(6) after a 2000ms delay.

## Config Items for 2i2 Gen 4

From `scarlett2_config_set_gen4_2i2`:

| Config | Descriptor Offset | Size (bits) | Activate | pbuf | mute |
|--------|------------------|-------------|----------|------|------|
| MSD_SWITCH | 0x49 (73) | 8 | 4 | 0 | 0 |
| DIRECT_MONITOR | 0x14A (330) | 8 | 16 | 1 | 0 |
| AUTOGAIN_SWITCH | 0x135 (309) | 8 | 10 | 1 | 0 |
| AUTOGAIN_STATUS | 0x137 (311) | 8 | — | 0 | 0 |
| AG_MEAN_TARGET | 0x131 (305) | 8 | 29 | 1 | 0 |
| AG_PEAK_TARGET | 0x132 (306) | 8 | 30 | 1 | 0 |
| PHANTOM_SWITCH | 0x48 (72) | 8 | 11 | 1 | **1** |
| INPUT_GAIN | 0x4B (75) | 8 | 12 | 1 | 0 |
| LEVEL_SWITCH | 0x3C (60) | 8 | 13 | 1 | **1** |
| SAFE_SWITCH | 0x147 (327) | 8 | 14 | 1 | 0 |
| AIR_SWITCH | 0x3E (62) | 8 | 15 | 1 | 0 |
| INPUT_SELECT_SWITCH | 0x14B (331) | 8 | 17 | 1 | 0 |
| INPUT_LINK_SWITCH | 0x14E (334) | 8 | 18 | 1 | 0 |
| DIRECT_MONITOR_GAIN | 0x2A0 (672) | 16 | 36 | 0 | 0 |

## Mute Flag Mechanism

Config items with `mute=1` (PHANTOM_SWITCH, LEVEL_SWITCH) use special transitional values to tell the firmware to mute audio output during the hardware change:

| Write Value | Meaning |
|-------------|---------|
| 0x02 | Set to ON (device temporarily mutes output) |
| 0x03 | Set to OFF (device temporarily mutes output) |

Reading back uses XOR decode: `(v ^ (v >> 1)) & 1`
- 0x00 → 0 (off, stable)
- 0x01 → 1 (on, stable)
- 0x02 → 1 (transitioning to on)
- 0x03 → 0 (transitioning to off)

This is NOT user-facing audio mute — it's an internal mechanism for safe hardware transitions.

## Notification Bitmasks (2i2 Gen 4)

Notifications arrive via USB interrupt endpoint (8 bytes, first 4 = LE32 bitmask):

| Bit | Handler | Description |
|-----|---------|-------------|
| 0x00000001 | ACK | Command acknowledgment |
| 0x00000008 | sync | Clock sync status changed |
| 0x00200000 | input_safe | Safe mode toggled |
| 0x00400000 | autogain | Autogain status changed |
| 0x00800000 | input_air | Air switch changed |
| 0x01000000 | direct_monitor | Direct monitor toggled |
| 0x02000000 | input_select | Input select changed |
| 0x04000000 | input_level | Level switch changed |
| 0x08000000 | input_phantom | Phantom power changed |
| 0x10000000 | (ignored) | Power status |
| 0x40000000 | input_gain | Input gain changed |
| 0x80000000 | (ignored) | Power status |

The `running` state controls processing:
- `running=0`: all notifications ignored
- `running=1`: only ACK processed (during init)
- `running=2`: all notifications processed (normal operation)

## Meter Levels Protocol

### GET_METER (raw: 0x00001001, SwRoot: 0x00010001)
```
Request (8 bytes):
  offset 0: u16 LE  pad = 0
  offset 2: u16 LE  num_meters
  offset 4: u32 LE  magic = 1

Response: num_meters x u32 LE values (raw meter levels)
```

Values are **12-bit**: range 0–4095 (0x0FFF). 4095 = full-scale (0 dBFS), 0 = silence. dB conversion: `20 × log10(value / 4095.0)`. The device returns u32 but only the lower 12 bits are populated — the Linux kernel driver truncates to u16 without masking and declares the ALSA control range as `min=0, max=4095, step=1` ([mixer_scarlett2.c `scarlett2_meter_ctl_info`](https://github.com/torvalds/linux/blob/master/sound/usb/mixer_scarlett2.c)).

FC2 polls this at ~22.5 Hz (every ~44ms). Response is 272 bytes = 8-byte header + 264 bytes = 66 meter values.

## Routing (MUX) Protocol

### GET_MUX (raw: 0x00003001)
```
Request (4 bytes):
  offset 0: u16 LE  num = 0
  offset 2: u16 LE  count (number of mux destinations)

Response: count x u32 LE values
  Each value: destination_id | (source_id << 12)
```

### SET_MUX (raw: 0x00003002)
```
Request:
  offset 0: u16 LE  pad = 0
  offset 2: u16 LE  table_index (0/1/2 for sample rate bands)
  offset 4: u32 LE  data[] (mux entries)
```

Port type IDs:
| ID | Type |
|----|------|
| 0x000 | NONE |
| 0x080 | ANALOGUE |
| 0x180 | SPDIF |
| 0x200 | ADAT |
| 0x300 | MIX |
| 0x600 | PCM |

### Physical Input → OS Audio Endpoint Mapping (2i2 4th Gen)

The firmware schema `device-specification` section defines the complete signal routing chain from physical inputs to OS-visible audio endpoints. This mapping is authoritative — it comes directly from the firmware, not from reverse engineering or inference.

**Signal chain:**

```
Physical Input 1 jack
  → Analogue 1 (eAnalogueInput_PreampCh1=0, router-pin 128)
  → MUX default routing
  → USB 1 destination (eUSBInput_Input1=0, router-pin 1536)
  → Left channel of "Analogue 1 + 2" in Windows WASAPI

Physical Input 2 jack
  → Analogue 2 (eAnalogueInput_PreampCh2=1, router-pin 129)
  → MUX default routing
  → USB 2 destination (eUSBInput_Input2=1, router-pin 1537)
  → Right channel of "Analogue 1 + 2" in Windows WASAPI
```

**Schema evidence** (`device-specification.physical-inputs`):

| Physical Input | Schema Name | Type | Router Pin | Controls Index |
|----------------|-------------|------|------------|----------------|
| Input 1 jack | Analogue 1 | analogue | 128 | 0 |
| Input 2 jack | Analogue 2 | analogue | 129 | 1 |

**Schema evidence** (`device-specification.destinations`, type=host):

| USB Capture | Schema Name | Router Pin | WASAPI Channel |
|-------------|-------------|------------|----------------|
| Channel 1 | USB 1 | 1536 | Left of "Analogue 1 + 2" |
| Channel 2 | USB 2 | 1537 | Right of "Analogue 1 + 2" |

**Independent confirmation** from three sources:
1. **Schema enums**: `eANALOGUE_INPUTS` (PreampCh1=0, PreampCh2=1) and `eUSB_INPUTS` (Input1=0, Input2=1) — consistent index ordering
2. **GET_METER**: meter[0]=Analogue In 1, meter[1]=Analogue In 2 — live signal confirms physical jack → channel mapping
3. **`selectedInput`** (descriptor offset 331): 0=Input 1, 1=Input 2 — front-panel Select button matches the same index scheme

**Note**: The 2i2 has only one stereo capture endpoint ("Analogue 1 + 2"). Larger interfaces (4i4, 8i6, etc.) have multiple endpoints and more complex MUX routing — the same schema structure would define their mappings, but the specific router pin values and endpoint groupings will differ.

## Flash Operations

### INFO_FLASH (0x00004000)
Response (16 bytes): `{ u32 size, u32 segment_count, u8[8] reserved }`

### INFO_SEGMENT (0x00004001)
Request: `{ u32 segment_num }`
Response (24 bytes): `{ u32 size, u32 flags, char[16] name }`

**Confirmed segment table** (Scarlett 2i2 4th Gen, firmware 2.0.2417.0):

| Segment | Name | Size | Flags | Used | Description |
|---------|------|------|-------|------|-------------|
| 0 | App_Gold | 256 KB | 0x0C | 204 KB | Encrypted XMOS firmware (golden image, see below) |
| 1 | App_Upgrade | 256 KB | 0x02 | 185 KB | Encrypted firmware update image (see below) |
| 2 | App_Disk | 192 KB | 0x40 | 100 KB | FAT12 filesystem — MSD "Easy Start" disk (see below) |
| 3 | App_Env | 64 KB | 0x40 | 165 B | Device environment metadata (see below) |
| 4 | App_Settings | 256 KB | 0x30 | 136 KB | Wear-leveling config journal (see below) |

Total: 1024 KB = 1 MB (matches INFO_FLASH flash_size). All 5 segments successfully read via READ_SEGMENT.

### ERASE_SEGMENT (0x00004002)
Request: `{ u32 segment_num, u32 pad=0 }`

Segment 0 (`App_Gold`) is always protected — erase requests are rejected by the firmware.

### GET_ERASE (0x00004003)
Request: `{ u32 segment_num, u32 pad=0 }`
Response: `{ u8 progress, u8 num_blocks }` — progress counts 1→num_blocks, then 0xFF = complete. Poll at 50ms intervals. Abort if progress goes backwards.

### WRITE_SEGMENT (0x00004004)
Request: `{ u32 segment_num, u32 offset, u32 pad=0, u8[] data }` (max 1012 bytes data per write; total request limit is `SCARLETT2_FLASH_RW_MAX` = 1024 minus 12-byte header)

### READ_SEGMENT (0x00004005)
Request: `{ u32 segment_num, u32 offset, u32 len }` (max 1024 bytes per page)

### Firmware Update Sequence (Small 4th Gen)

Source: [scarlett2 CLI tool](https://github.com/geoffreybennett/scarlett2) (`main.c`).

```
1. ERASE_SEGMENT(App_Settings)     ← erase user config
   → poll GET_ERASE until 0xFF
2. ERASE_SEGMENT(App_Upgrade)      ← erase old firmware
   → poll GET_ERASE until 0xFF
3. WRITE_SEGMENT(App_Upgrade, offset, data) × N   ← write new firmware in 1012-byte chunks
4. REBOOT                          ← device disconnects, boots from App_Upgrade
```

The firmware binary is validated before flashing: SHA-256 of payload must match the SCARLETT header hash, and the USB PID must match the connected device. Segment 0 (`App_Gold`) is never touched — it serves as a factory recovery image.

**App_Gold contents** (segment 0, confirmed on Scarlett 2i2 4th Gen):

The App_Gold segment contains the **factory firmware image** for the XMOS XU216 processor. The firmware is encrypted — no readable strings are present anywhere in the 208,436-byte payload.

```
Header (first 32 bytes):
  0x0000: 06 90 00 00  — XMOS boot instruction (branch/jump)
  0x0004: AC CE 00 09  — unknown (load address or checksum?)
  0x0008: 00 F4 C0 00  — code entry point (0x00C0F400?)
  0x000C: 00 00 10 00  — flash size = 1,048,576 (1 MB)
  0x0010: B0 00 00 00  — header size = 176 bytes
  0x0014: 00 ... 00    — zeros (padding to offset 0x70)
  0x0038: 08 00 00 00  — unknown (core count? = 8)
```

Code begins at offset 0x70 (after the 176-byte header). The high-entropy data with no readable strings is consistent with AES-encrypted firmware, matching the analysis in [14-firmware-binary-analysis.md](14-firmware-binary-analysis.md). The `0x0C` segment flags likely mean read-only + boot source.

**App_Upgrade contents** (segment 1, confirmed on Scarlett 2i2 4th Gen):

The App_Upgrade segment contains a **different firmware image** — 189,536 bytes, ~19KB smaller than App_Gold. This is the most recently applied firmware update.

```
Header (first 16 bytes):
  0x0000: 7B 8A FF 00  — different magic from App_Gold (not XMOS boot format)
  0x0004: 44 1E 81 C1  — unknown
  0x0008: E3 94 BF 26  — unknown
  0x000C: 02 00 00 00  — version = 2 (?)
```

The header format differs completely from App_Gold, suggesting this is a **DFU update package** rather than a raw XMOS boot image. The `0x02` segment flags (vs `0x0C` for App_Gold) confirm this is a writable staging area. Also fully encrypted — no readable strings.

**App_Disk contents** (segment 2, confirmed on Scarlett 2i2 4th Gen):

The App_Disk segment is a **FAT12 filesystem** containing the "Easy Start" USB mass storage device content. When MSD mode is enabled (`MSD_SWITCH` at descriptor offset 73, activate value 4), the Scarlett presents this filesystem as a removable USB drive.

- **101,886 bytes** used of 192 KB
- **MBR** at offset 0x0000: standard x86 boot code, single partition, `55 AA` boot signature
- **Partition type**: 0x0E (FAT16B LBA) — despite actual filesystem being FAT12
- **Partition start**: LBA 63 → byte offset 0x7E00

**FAT12 boot sector** (at offset 0x7E00):

| Field | Value |
|-------|-------|
| OEM name | `MSDOS5.0` |
| Bytes per sector | 512 |
| Sectors per cluster | 8 |
| Reserved sectors | 6 |
| Number of FATs | 2 |
| Root directory entries | 512 |
| Total sectors | 321 |
| Sectors per FAT | 1 |
| Volume label | `SCARLETT` |
| FS type string | `FAT12` |

**File listing** (FAT12 directory entries parsed and contents extracted):

| Filename | Size | Type | Description |
|----------|------|------|-------------|
| `AUTORUN.INF` | 114 B | Text | `[autorun]` — `icon=Scarlett.ico`, `label=Scarlett` |
| `CLICKHER.URL` | 171 B | Text | `[InternetShortcut]` — `URL=https://api.focusrite-novation.com/register?method=urlfile&upn=00000000000000` |
| `READMEFO.HTM` | 1,871 B | HTML | "Easy Start" welcome page with setup instructions and FAQ |
| `SCARLETT.ICO` | 14,846 B | Binary | Windows multi-resolution icon (5 sizes, 8-bit color) |
| `VOLUME~1.ICN` | 10,486 B | Binary | macOS ICNS icon (contains embedded 256×256 PNG) |
| `_VOLUM~1.ICN` | 4,096 B | Binary | macOS resource fork metadata for VOLUME~1.ICN |
| `_65F6~1` | 4,096 B | Binary | Orphaned macOS resource fork (cluster out of bounds in trimmed data) |

**AUTORUN.INF contents:**
```ini
; Scarlett=
; Serial Number=00000000000000

[autorun]
icon=Scarlett.ico
label=Scarlett
```

**CLICKHER.URL contents:**
```ini
[InternetShortcut]
URL=https://api.focusrite-novation.com/register?method=urlfile&upn=00000000000000
IDList=
HotKey=0
[{000214A0-0000-0000-C000-000000000046}]
Prop3=19,11
```

**READMEFO.HTM summary:** HTML body with Arial font. Contains welcome message (*"Welcome to the Focusrite Easy Start tool! Thanks for purchasing your Scarlett..."*), link to `register?method=readme&upn=00000000000000`, FAQ explaining MSD mode and how to install control software to exit Easy Start, and a `<hr>` device info footer with placeholder serial.

**Key observations:**
- All files contain `00000000000000` for serial/UPN — matches `App_Env.url_str`. Firmware likely patches these at runtime.
- Both Windows (ICO, INF, URL) and macOS (ICNS, resource forks) assets are included.
- Registration URLs use different `method` parameters (`urlfile`, `readme`) to track user entry point.
- The two `_` prefixed entries are macOS `.DS_Store`-style artifacts from the build process.

**App_Env contents** (segment 3, confirmed on Scarlett 2i2 4th Gen):

The App_Env segment stores plain-text key=value pairs (newline-separated). Only ~165 bytes used of the 64 KB allocation; the rest is empty flash.

```
serial_str=S2G6HVK563186A
pcba_sn=Y250530057501
powercycles=0x0000003f
totalsec=0x000c5470
url_str=api.focusrite-novation.com/register?method=usb&upn=00000000000000
```

| Key | Description |
|-----|-------------|
| `serial_str` | Device serial number |
| `pcba_sn` | PCB assembly serial number |
| `powercycles` | Lifetime USB power-on cycle count (hex) |
| `totalsec` | Lifetime powered-on seconds (hex) |
| `url_str` | Focusrite product registration URL |

**App_Settings contents** (segment 4, confirmed on Scarlett 2i2 4th Gen):

The App_Settings segment is a **flash wear-leveling journal** containing sequential config snapshots. The firmware appends a new record on every DATA_CMD(6) / NVRAM save, spreading flash write wear across the segment. On boot, the firmware reads the **last valid record** to restore device configuration.

**Journal structure:**

```
Record separator:  0xA5C35A3C (u32 LE magic marker)
Record header:     FE FF D0 02
                   ^^^^ ^^^^^ descriptor size (u16 LE = 0x02D0 = 720)
                   ||||| sentinel/BOM
Record body:       720 bytes of APP_SPACE descriptor (full snapshot)
Optional tail:     routing/mux table blocks (separated by magic markers)
```

The first record in the segment has no leading magic marker (starts at offset 0 with `00 00 00 00 FE FF D0 02`). All subsequent records are preceded by `0xA5C35A3C`.

**Confirmed journal metrics** (after 63 power cycles, ~9.4 days runtime):

| Metric | Value |
|--------|-------|
| Total snapshots | 191 |
| Bytes per record | ~728 (header + 720 descriptor + magic) |
| Journal used | ~136 KB of 256 KB |
| Free space | ~120 KB (~165 more records before full) |

**Config evolution across slots:**

Comparing the first and last (191st) snapshots reveals how firmware-managed state accumulates over the device's lifetime:

| Field | First snapshot | Last snapshot | Interpretation |
|-------|---------------|---------------|----------------|
| `directLEDValues[1-9]` | `0xFF000000` (RED) | `0x00000000` | Factory test pattern cleared |
| `directLEDValues[27]` (Select) | `0x00000000` | `0x70808800` | Firmware wrote calibrated white |
| `directLEDValues[31]` (Auto) | `0x00000000` | `0x70808800` | Firmware wrote calibrated white |
| `directLEDValues[35]` (Select 2) | `0x00000000` | `0x70808800` | Firmware wrote calibrated white |
| `directLEDValues[39]` (USB) | `0x00000000` | `0x00380000` | Firmware wrote calibrated green |
| `LEDthresholds` | All zeros | Populated | Metering gradient calibrated |

This confirms finding 43: the firmware writes calibrated default colors (`0x70808800` = white, `0x00380000` = green) to cache-dependent button LED positions in `directLEDValues` during normal operation. These are ground-truth values, not approximations.

**Routing data blocks:**

The first record additionally includes routing/mux configuration blocks appended after the descriptor, separated by magic markers. Each block contains u32 LE routing entries (format matches SET_MUX data). Most subsequent records contain only the descriptor — routing data is only saved when the routing table changes.

## Schema (Devmap) Protocol

### INFO_DEVMAP (raw: 0x0080000c, SwRoot: 0x000C0800)

Request: no payload.
Response (4 bytes after 8-byte transact header): `{ u16 unknown, u16 config_len }`

- `config_len` is the **base64 content length** in bytes (LE), NOT the allocated devmap size.
- The Scarlett 2i2 4th Gen (fw 2.0.2417.0) returns `config_len = 5333` (0x14D5).
- Page count: `ceil(config_len / 1024)`. For the 2i2 this is 6 pages.

**Confirmed response bytes** (Scarlett 2i2 4th Gen, 32-byte read):
```
00 00 00 00  04 00 00 00  00 00 D5 14  00 00 00 00
00 00 00 00  00 00 00 00  00 00 00 00  00 00 00 00
```
- Bytes 0-7: transact header (zeroed)
- Bytes 8-9: `00 00` — unknown (u16 LE = 0, possibly a version or flags field)
- Bytes 10-11: `D5 14` — config_len (u16 LE = 0x14D5 = 5333)
- Bytes 12+: zeroed padding

### GET_DEVMAP (raw: 0x0080000d, SwRoot: 0x000D0800)

Request: `{ u32 block_number }` (0-indexed, LE)
Response: 8-byte transact header + up to 1024 bytes of payload.

Payload is a segment of the **base64-encoded, zlib-compressed JSON** schema.
Concatenate all page payloads (stripping the 8-byte header from each),
then truncate to `config_len` bytes from INFO_DEVMAP.

### Decoding Pipeline

1. **Concatenate pages**: Read `ceil(config_len / 1024)` pages, strip 8-byte headers, truncate to `config_len`.
2. **Strip trailing nulls**: Last page may be zero-padded beyond the base64 content.
3. **Base64 decode**: Standard base64 (A-Za-z0-9+/=). Result is zlib-compressed data (magic `78 DA`).
4. **Zlib decompress**: Yields the JSON schema (~25KB for the 2i2).

**Verified on hardware**: Scarlett 2i2 4th Gen, firmware 2.0.2417.0.
FC2 reads 6 blocks (6 × 1032 = 6192 bytes with 8-byte headers). Decompressed: ~25KB JSON.

## Autogain Status Values (Gen 4)

| Value | Status |
|-------|--------|
| 0 | Running |
| 1 | Success |
| 2 | SuccessDRover (dynamic range over threshold) |
| 3 | WarnMinGainLimit |
| 4 | FailDRunder (dynamic range under) |
| 5 | FailMaxGainLimit |
| 6 | FailClipped |
| 7 | Cancelled |
| 8 | Invalid |

## IOCTL_NOTIFY Bitmask (Complete — 2i2 Gen 4)

From the firmware schema (`eDEV_FCP_NOTIFY_MESSAGE_TYPE`). Returned in bytes 4-7 of the 16-byte IOCTL `0x0022200C` response.

| Bitmask | Hex | Event | Confirmed |
|---------|-----|-------|-----------|
| 0x00200000 | bit 21 | FCP_NOTIFY_CLIPSAFE | — |
| 0x00400000 | bit 22 | FCP_NOTIFY_AUTOGAIN_CHANGE | — |
| 0x00800000 | bit 23 | FCP_NOTIFY_INPUT_AIR_CHANGE | Yes (Air button) |
| 0x01000000 | bit 24 | FCP_NOTIFY_DIRECT_MONITORING_CHANGE | Yes (DM toggle) |
| 0x02000000 | bit 25 | FCP_NOTIFY_CHANNEL_LINKING_CHANGE / FCP_NOTIFY_SELECT_PREAMP | Yes (Select button) |
| 0x04000000 | bit 26 | FCP_NOTIFY_INST_INPUT_CHANGE | Yes (Inst button, also 0x44000000) |
| 0x08000000 | bit 27 | FCP_NOTIFY_PHANTOM_POWER_CHANGE | — |
| 0x10000000 | bit 28 | FCP_NOTIFY_USB2_CHANGE | — |
| 0x20000000 | bit 29 | FCP_NOTIFY_TRS_INPUT_CHANGE | — |
| 0x40000000 | bit 30 | FCP_NOTIFY_INPUT_GAIN_CHANGE | — |
| 0x80000000 | bit 31 | FCP_NOTIFY_LOW_VOLTAGE_DETECT | — |

> **Note**: `FCP_NOTIFY_CHANNEL_LINKING_CHANGE` and `FCP_NOTIFY_SELECT_PREAMP` share the same bitmask value (0x02000000). The firmware fires this notification for both events.

## USB Product IDs (All Focusrite Models)

Source: [alsa-scarlett-gui `hardware.h`](https://github.com/geoffreybennett/alsa-scarlett-gui). VID is always `0x1235`.

| Generation | Model | PID | Driver Type |
|------------|-------|-----|-------------|
| 1st Gen | Scarlett 6i6 | 0x8012 | hwdep |
| 1st Gen | Scarlett 8i6 | 0x8002 | hwdep |
| 1st Gen | Scarlett 18i6 | 0x8004 | hwdep |
| 1st Gen | Scarlett 18i8 | 0x8014 | hwdep |
| 1st Gen | Scarlett 18i20 | 0x800C | hwdep |
| 2nd Gen | Scarlett 6i6 | 0x8203 | hwdep |
| 2nd Gen | Scarlett 18i8 | 0x8204 | hwdep |
| 2nd Gen | Scarlett 18i20 | 0x8201 | hwdep |
| Clarett USB | 2Pre | 0x8206 | hwdep |
| Clarett USB | 4Pre | 0x8207 | hwdep |
| Clarett USB | 8Pre | 0x8208 | hwdep |
| Clarett+ | 2Pre | 0x820A | hwdep |
| Clarett+ | 4Pre | 0x820B | hwdep |
| Clarett+ | 8Pre | 0x820C | hwdep |
| 3rd Gen | Scarlett Solo | 0x8211 | hwdep |
| 3rd Gen | Scarlett 2i2 | 0x8210 | hwdep |
| 3rd Gen | Scarlett 4i4 | 0x8212 | hwdep |
| 3rd Gen | Scarlett 8i6 | 0x8213 | hwdep |
| 3rd Gen | Scarlett 18i8 | 0x8214 | hwdep |
| 3rd Gen | Scarlett 18i20 | 0x8215 | hwdep |
| Vocaster | One | 0x8216 | hwdep |
| Vocaster | Two | 0x8217 | hwdep |
| **4th Gen** | **Scarlett Solo** | **0x8218** | **hwdep** |
| **4th Gen** | **Scarlett 2i2** | **0x8219** | **hwdep** |
| **4th Gen** | **Scarlett 4i4** | **0x821A** | **hwdep** |
| **4th Gen** | **Scarlett 16i16** | **0x821B** | **socket** |
| **4th Gen** | **Scarlett 18i16** | **0x821C** | **socket** |
| **4th Gen** | **Scarlett 18i20** | **0x821D** | **socket** |

> **Critical**: Big 4th Gen models (16i16, 18i16, 18i20) use "FCP Socket" (Unix domain socket IPC) on Linux instead of kernel hwdep/TRANSACT. They have an ESP32 chip for WiFi/Bluetooth alongside the XMOS main processor. On Windows, they likely still use SwRoot (unverified). See [16-multi-model-mute-design.md](16-multi-model-mute-design.md).

## Device Info (2i2 Gen 4)

| Property | Value |
|----------|-------|
| USB VID:PID | 0x1235:0x8219 |
| Minimum firmware | 2115 |
| Parameter buffer addr | 0xFC |
| MSD enable value | 0x02 |

## Sources

- [torvalds/linux — mixer_scarlett2.c](https://github.com/torvalds/linux/blob/master/sound/usb/mixer_scarlett2.c)
- [geoffreybennett/scarlett2-firmware](https://github.com/geoffreybennett/scarlett2-firmware)
- [geoffreybennett/alsa-scarlett-gui](https://github.com/geoffreybennett/alsa-scarlett-gui)

---
[← TRANSACT Protocol](12-transact-protocol-decoded.md) | [Index](README.md) | [Firmware Binary Analysis →](14-firmware-binary-analysis.md)
