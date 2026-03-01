# Design Note: Multi-Model Mute Indicator Architecture

## Context

Focusmute's mute indicator monitors the system capture device's mute state and lights up LED halos on the Scarlett interface. On the 2i2, this is straightforward — one stereo capture endpoint, two physical inputs, known LED mapping. On larger interfaces (4i4, 16i16, 18i16, 18i20), the architecture becomes significantly more complex due to multiple capture endpoints, user-configurable routing, and unvalidated LED layouts.

This document captures the design constraints and future work needed to properly support the full Scarlett 4th Gen lineup.

## Current Architecture (2i2 / Solo)

### Signal Chain

```
Physical Input 1 jack
  → Analogue 1 (router-pin 128)
  → MUX (hardwired) → USB 1 (router-pin 1536)
  → Left channel of "Analogue 1 + 2" in Windows WASAPI

Physical Input 2 jack
  → Analogue 2 (router-pin 129)
  → MUX (hardwired) → USB 2 (router-pin 1537)
  → Right channel of "Analogue 1 + 2" in Windows WASAPI
```

### Mute Detection

- Focusmute monitors the **default capture device** via WASAPI `IAudioEndpointVolume::GetMute()` (Windows) or PulseAudio source mute (Linux)
- Mute is a single boolean — the entire stereo endpoint is muted or not
- No per-channel mute exists at the OS level

### LED Indication

The `mute_inputs` config controls which number indicator LEDs light up:

| Config | Behavior | Mechanism |
|--------|----------|-----------|
| `all` | All input number LEDs glow mute color | `directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8) per LED |
| `1` | Only Input 1 number LED | `directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8) |
| `2` | Only Input 2 number LED | `directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8) |
| `1,2` | Both input number LEDs | `directLEDColour` + `directLEDIndex` + DATA_NOTIFY(8) per LED |

### Why This Works on the 2i2

1. **One capture endpoint** — "Analogue 1 + 2" is the only capture device. The default capture device IS the Scarlett, always.
2. **Hardwired routing** — Input 1 always maps to USB channel 1, Input 2 to USB channel 2. FC2 does not expose routing controls on the 2i2.
3. **Known LED mapping** — 40 LEDs, index mapping confirmed via physical testing (indices 0-7 = Input 1, 8-15 = Input 2, 16-26 = Output, 27-39 = Buttons).
4. **`mute_inputs` is purely visual** — it controls which halos show the indicator, not what gets muted.

## Multi-Endpoint Models (4i4+)

### Problem 1: Multiple Capture Endpoints

Larger interfaces expose multiple capture endpoints in Windows:

| Model | Expected Capture Endpoints | Physical Inputs |
|-------|---------------------------|----------------|
| Solo | "Analogue 1 + 2" (1 stereo) | 1 mic + 1 instrument |
| 2i2 | "Analogue 1 + 2" (1 stereo) | 2 mic/instrument |
| 4i4 | "Analogue 1 + 2", "Analogue 3 + 4" (2 stereo) | 2 mic + 2 line |
| 16i16 | 8 stereo endpoints | 8 mic/line + 8 ADAT |
| 18i16 | 9 stereo endpoints | 8 mic/line + S/PDIF + ADAT |
| 18i20 | 10 stereo endpoints | 8 mic/line + S/PDIF + 10 ADAT |

**Impact**: Muting "Analogue 1 + 2" does NOT mute "Analogue 3 + 4". They are independent audio devices in Windows. Focusmute currently monitors the default capture device — if the user's mic is on a non-default endpoint, Focusmute won't detect the mute at all.

### Problem 2: Which Endpoint Has the Mic?

On the 2i2, the default capture device is the only option. On a 4i4:

- User's mic might be in Input 3 → "Analogue 3 + 4" endpoint
- Their default capture device might be "Analogue 1 + 2" (Windows default)
- Discord/Zoom might be configured to use "Analogue 3 + 4" explicitly
- Focusmute watches the default → misses the mute entirely

**Possible solutions**:

1. **Let the user pick which endpoint to monitor** — config option like `capture_device = "Analogue 3 + 4"`. Simple, explicit, but requires user knowledge.
2. **Monitor all Scarlett endpoints** — detect all capture endpoints belonging to the connected Scarlett, watch all of them. More complex but automatic.
3. **Monitor the endpoint the app is using** — hook into the audio session to see which endpoint Discord/Zoom has open. Very complex, invasive, and fragile.

Option 1 is the most practical for an initial implementation. Option 2 is the ideal UX.

### Problem 3: Endpoint → Physical Input Mapping

Even if we know WHICH endpoint was muted, we need to trace it back to a physical input to know which LED halo to light up. This requires:

1. **Endpoint name → USB capture channel pair**: "Analogue 3 + 4" → USB channels 3 and 4. This mapping comes from the USB Audio Class descriptors and is deterministic per model, but not currently parsed by Focusmute.

2. **USB capture channel → MUX source**: The MUX routing table maps USB capture channels (destinations) to analogue inputs (sources). On the 2i2 this is hardwired; on the 4i4+ it's **user-configurable via Focusrite Control 2**.

3. **MUX source → physical input**: The `device-specification.physical-inputs` schema array maps analogue source names/router-pins to physical jack positions.

4. **Physical input → LED halo indices**: The LED index mapping (which indices correspond to which input halo) must be known per model.

**The full chain**:

```
"Analogue 3 + 4" (Windows endpoint name)
  → USB Capture channels 3 + 4
  → GET_MUX: USB 3 ← Analogue 3 (router-pin 130), USB 4 ← Analogue 4 (router-pin 131)
  → Physical Input 3 and Input 4
  → LED halo indices for Input 3 and Input 4 (from model profile or predict)
```

### Problem 4: User-Configurable MUX Routing

On 4i4+ models, Focusrite Control 2 exposes a routing matrix. Users can change which physical input feeds which USB capture channel. For example:

- Default: Input 1 → USB 1, Input 2 → USB 2, Input 3 → USB 3, Input 4 → USB 4
- User changes: Input 3 → USB 1 (for a specific recording setup)

If Focusmute assumes the default routing, it would light up the wrong halo. Solutions:

1. **Read MUX on startup**: `GET_MUX` (SwRoot 0x00010003) returns the current routing table. Parse it to build the actual input → USB channel mapping.
2. **Re-read on routing changes**: IOCTL_NOTIFY bitmask should include a bit for routing changes (the Linux driver handles MUX notifications). On notification, re-read the MUX table and update the mapping.
3. **Ignore routing changes**: Assume default routing, document the limitation. Simplest, covers 99% of users.

### Problem 5: Unknown LED Layouts

We only have a confirmed LED mapping for the 2i2 (40 LEDs, physically tested). Other models:

| Model | LED Count (predicted) | Halo Layout | Status |
|-------|----------------------|-------------|--------|
| Solo | ~24? | 1 input halo + output | Unvalidated |
| 2i2 | 40 | 2 input halos + output | **Confirmed** |
| 4i4 | ~56? | 4 input halos + output | Unvalidated |
| 16i16+ | Unknown | Unknown | No data |

The `predict` command (and `layout.rs`) attempts to infer LED layouts from firmware schemas by counting inputs, outputs, and `kMAX_NUMBER_LEDS`. This has been tested against the 2i2 schema and produces the correct layout, but has never been validated on other hardware.

**What's needed**: Someone with a 4i4 (or other model) to run `focusmute-cli probe --dump-schema` and `focusmute-cli map` to capture the schema and confirm the physical LED mapping.

## Proposed Multi-Model Architecture

### Phase 1: Config-Driven Endpoint Selection

Add a `capture_device` config option:

```toml
# Default: monitor the system default capture device
capture_device = "default"

# Explicit: monitor a specific endpoint by name
capture_device = "Analogue 3 + 4"
```

This solves Problem 2 without any protocol work. The user explicitly tells Focusmute which endpoint to watch. Combined with `mute_inputs`, this gives full control:

```toml
capture_device = "Analogue 3 + 4"
mute_inputs = "3,4"
```

### Phase 2: Schema-Driven Input Mapping

Use the firmware schema's `device-specification` to automatically build the mapping:

1. On connect, extract the schema (already implemented via `probe`/`extract_schema`)
2. Parse `physical-inputs` → build router-pin-to-input-index map
3. Parse `destinations` (type=host) → build router-pin-to-USB-channel map
4. The LED layout is already predicted from the schema via `predict_layout()`

This gives the automatic mapping without reading the MUX table — it uses the default routing implied by the schema ordering.

### Phase 3: MUX-Aware Routing

For full correctness when users have custom routing:

1. Read `GET_MUX` on startup to get actual source→destination mapping
2. Register for IOCTL_NOTIFY routing change events
3. On change, re-read MUX and update the input→LED mapping
4. This is only needed for 4i4+ models where routing is user-configurable

### Phase 4: Multi-Endpoint Monitoring

For the ideal "just works" experience:

1. Enumerate all capture endpoints belonging to the connected Scarlett (match by device path or Focusrite VID)
2. Monitor mute state on all of them simultaneously
3. When any endpoint is muted, light up the halos corresponding to its physical inputs
4. When unmuted, restore those halos

This removes the need for `capture_device` config entirely — Focusmute would automatically detect which endpoint was muted and light up the correct halos.

## Data We Have vs. Data We Need

| Data Point | 2i2 Status | 4i4+ Status |
|------------|-----------|-------------|
| Firmware schema | Extracted, parsed | Need hardware to extract |
| Physical input list | From schema | From schema (once extracted) |
| USB endpoint names | Known ("Analogue 1 + 2") | Need to enumerate on hardware |
| Default MUX routing | From schema ordering | From schema ordering |
| Actual MUX routing | Readable via GET_MUX | Readable via GET_MUX |
| LED count | 40 (confirmed) | From schema `kMAX_NUMBER_LEDS` |
| LED index mapping | Confirmed via physical test | Predicted by `predict_layout()`, unvalidated |
| Endpoint → USB channel mapping | Trivial (1 endpoint) | Need USB Audio Class descriptor parsing or naming convention |
| Jack detection (`inputTRSPresent`) | **TRS-only** — does NOT detect XLR (hardware-confirmed) | Presumably same limitation |

## Key Insight

The `mute_inputs` config option currently means "which halos to light up" and is decoupled from "which endpoint to monitor" (always the default). For multi-endpoint models, these two concerns need to be connected: the monitored endpoint determines which physical inputs are relevant, and those inputs determine which halos to light up. The user shouldn't have to manually keep `mute_inputs` in sync with their audio routing — the system should derive it.

The firmware schema provides everything needed to build this mapping automatically. The main blocker is hardware access for validation on non-2i2 models.

**Jack detection limitation**: `inputTRSPresent` only detects TRS insertion — XLR connections are invisible to firmware (hardware-confirmed on 2i2). This rules out auto-detecting which inputs have mics plugged in for the most common scenario (XLR condenser mic). The config-driven `mute_inputs` approach remains necessary. `inputTRSPresent` is still useful as a secondary signal (e.g., on a 4i4, auto-include TRS instrument inputs alongside a config-specified mic input), but it cannot be the sole source of truth for mute indication.

## Addendum: Big 4th Gen Communication Architecture

> Source: [alsa-scarlett-gui](https://github.com/geoffreybennett/alsa-scarlett-gui) commits `afdebf9c`, `5201eea3`, `d1daa1a8` (Feb 2026).

### Two Driver Types in the 4th Gen Lineup

The small 4th Gen models (Solo, 2i2, 4i4) and the big 4th Gen models (16i16, 18i16, 18i20) use **fundamentally different communication paths on Linux**:

| Models | PID Range | Linux Driver | Communication |
|--------|-----------|-------------|---------------|
| Solo, 2i2, 4i4 | 0x8218–0x821A | `hwdep` | Kernel IOCTL / TRANSACT protocol |
| 16i16, 18i16, 18i20 | 0x821B–0x821D | `socket` | FCP Socket (Unix domain socket IPC) |

The `hwdep` path is our fully-documented TRANSACT protocol: `DeviceIoControl()` with IOCTL `0x00222008`, session token, `[token:u64][cmd:u32][pad:u32][payload]` format.

The `socket` path uses a separate server process that handles USB communication. The client sends structured binary messages over a Unix domain socket.

### FCP Socket Protocol

**Framing**:
```
[magic:u8][msg_type:u8][payload_length:u32][payload...]
```

| Field | Client→Server | Server→Client |
|-------|--------------|---------------|
| magic | `0x53` ('S') | `0x73` ('s') |

**Request types**:
| Code | Name | Purpose |
|------|------|---------|
| 0x0001 | REBOOT | Reboot device |
| 0x0002 | CONFIG_ERASE | Erase device settings |
| 0x0003 | APP_FIRMWARE_ERASE | Erase XMOS application firmware |
| 0x0004 | APP_FIRMWARE_UPDATE | Upload new XMOS firmware |
| 0x0005 | ESP_FIRMWARE_UPDATE | Upload new ESP32 firmware |

**Response types**: VERSION (0x00), SUCCESS (0x01), ERROR (0x02), PROGRESS (0x03).

### ESP32 in Big 4th Gen

The big models have an **ESP32 chip** for WiFi/Bluetooth alongside the main XMOS processor. This means:
- Two separate firmware images (XMOS app + ESP32)
- Firmware updates require a multi-step "leapfrog" process
- An `ESP Firmware Version` ALSA control (4-valued version number)
- The firmware container format `"SCARLBOX"` holds up to 3 sections: `"SCARLET4"` (XMOS), `"SCARLESP"` (ESP32), `"SCARLEAP"` (leapfrog bootloader)

### Impact on Multi-Model Support

This has **significant implications** for the Phase 2-4 architecture proposed above:

1. **On Linux**: Focusmute currently uses `nusb` for raw USB access. This works for hwdep models (Solo, 2i2, 4i4) because they respond to the same TRANSACT protocol. For socket models (16i16+), Focusmute would need to either:
   - Use ALSA controls (the `scarlett2` kernel driver abstracts the socket/hwdep difference)
   - Implement FCP Socket client support directly
   - Or only support hwdep models initially

2. **On Windows**: The big models likely still communicate through `FocusriteUsbSwRoot.sys` (the Windows driver doesn't distinguish hwdep vs socket — that's a Linux kernel concept). This needs verification with hardware.

3. **Schema extraction**: `GET_DEVMAP` should work identically on all models (it's a TRANSACT command). The schema format is the same — only the contents differ (more inputs, outputs, mixer channels).

4. **MUX routing**: `GET_MUX` is also a TRANSACT command, so the same code should work. The difference is `MAX_MUX_IN` = 53 for the 18i20 (vs 6 for the 2i2).

**Recommendation**: For initial multi-model support, target hwdep models only (Solo, 2i2, 4i4). Big model support requires either Linux ALSA integration or Windows hardware verification. Document the limitation.

---
[← Build & Packaging](15-build-and-packaging.md) | [Index](README.md)
