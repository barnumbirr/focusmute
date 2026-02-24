# Direct USB Control Feasibility Assessment

## Verdict: NOT feasible on Windows

Direct USB control of the Scarlett 2i2 4th Gen from Windows userspace is **blocked** by the driver architecture.

## Why It Doesn't Work

### 1. Focusrite Claims the Entire USB Device

The Windows driver INF (`FocusriteCustom.inf`) matches on the **whole USB device**:
```
USB\VID_1235&PID_8219
```

NOT on individual interfaces (no `&MI_xx` suffix). This means `FocusriteUsb.sys` claims everything — there is no unclaimed interface for WinUSB/libusb to attach to.

### 2. Windows Doesn't Support Driver Detachment

Unlike Linux, Windows has NO `detach_kernel_driver()` mechanism. You cannot temporarily steal an interface from a running driver.

### 3. Replacing the Driver Breaks Audio

Using Zadig to replace FocusriteUsb.sys with WinUSB would give you raw USB access but **destroy all audio/MIDI functionality**.

### 4. Filter Driver is Impractical

A Windows kernel filter driver could intercept/inject USB control transfers, but:
- Requires kernel-mode code signing (EV certificate, ~$400/year)
- Extremely complex to develop and debug
- Security/stability risk

## What the USB Protocol Looks Like (for reference)

From the Linux kernel driver `mixer_scarlett2.c`:

### Transport
- USB control transfers on endpoint 0
- Class-specific request type (`bmRequestType = 0x21` out, `0xA1` in)
- `bRequest = 2` (send), `bRequest = 3` (receive)

### Packet Format
```
Bytes 0-3:   cmd    (command ID, le32)
Bytes 4-5:   size   (payload size, le16)
Bytes 6-7:   seq    (sequence number, le16)
Bytes 8-11:  error  (error code, le32)
Bytes 12-15: pad    (zero, le32)
Bytes 16+:   data[] (variable payload)
```

### Key Commands
| Command | ID | Purpose |
|---------|-----|---------|
| GET_DATA | 0x00800000 | Read config memory |
| SET_DATA | 0x00800001 | Write config memory |
| DATA_CMD | 0x00800002 | Activate config change |
| GET_METER | 0x00001001 | Read level meters |
| SET_MIX | 0x00002002 | Set mixer levels |
| SET_MUX | 0x00003002 | Set routing |

### 2i2 4th Gen Config Space
| Config Item | Offset | Size | Activate | Description |
|-------------|--------|------|----------|-------------|
| LEVEL_SWITCH | 0x3C | 1 | 13 | Inst/Line |
| AIR_SWITCH | 0x3E | 1 | 15 | Air mode |
| PHANTOM_SWITCH | 0x48 | 1 | 11 | 48V phantom |
| MSD_SWITCH | 0x49 | 1 | 4 | Mass Storage mode |
| INPUT_GAIN | 0x4B | 1 | 12 | Preamp gain |
| AG_MEAN_TARGET | 0x131 | 1 | 29 | Autogain target |
| AG_PEAK_TARGET | 0x132 | 1 | 30 | Autogain peak |
| AUTOGAIN_SWITCH | 0x135 | 1 | 10 | Autogain on/off |
| AUTOGAIN_STATUS | 0x137 | 1 | 0 | Autogain status (read-only) |
| SAFE_SWITCH | 0x147 | 1 | 14 | Safe mode |
| DIRECT_MONITOR | 0x14A | 1 | 16 | Direct monitor |
| INPUT_SELECT | 0x14B | 1 | 17 | Input select |
| INPUT_LINK | 0x14E | 1 | 18 | Channel link |
| DIRECT_MONITOR_GAIN | 0x2A0 | 2 | 36 | DM mix gains |

### Gen 4 Parameter Buffer
Gen 4 uses an indirection mechanism (param_buf_addr = 0xFC):
1. Write channel index to offset 0xFD
2. Write value to offset 0xFC
3. Send DATA_CMD with activate number

### LED Halo in Config Space

> **CORRECTION**: The Linux kernel driver's config items do NOT include LED entries for 4th Gen. However, the firmware descriptor schema (extracted via GET_DEVMAP) DOES contain a complete LED API — `enableDirectLEDMode` (offset 77), `directLEDValues` (offset 92), `LEDcolors` (offset 384), `brightness` (offset 711). LED control is fully working via SET_DESCR + DATA_NOTIFY. See [09-led-control-api-discovery.md](09-led-control-api-discovery.md).

**NO halo-related entries in the Linux driver's config item table for 4th Gen.** The 3rd Gen had:
- GAIN_HALO_ENABLE (offset 0x16, activate 9)
- GAIN_HALO_LEDS (offset 0x17, activate 9)
- GAIN_HALO_LEVELS (offset 0x1A, activate 11)

These config items were removed for 4th Gen, but the LED functionality moved to the descriptor schema (not implemented in the Linux driver).

## Rust USB Crates (for completeness)

| Crate | Status on Windows |
|-------|-------------------|
| `rusb` | Requires WinUSB driver — won't work with Focusrite driver |
| `nusb` | Requires WinUSB driver — won't work with Focusrite driver |

## Conclusion

Direct USB (via libusb/WinUSB) is a dead end on Windows. The viable approaches are:
1. **IOCTL through FocusriteUsbSwRoot.sys** (TRANSACT protocol via `\pal` device path) — **THIS IS WHAT WORKS.** See [10-usb-access-investigation.md](10-usb-access-investigation.md) and [12-transact-protocol-decoded.md](12-transact-protocol-decoded.md).
2. **Windows WASAPI** (for system-level mute detection — doesn't need USB access at all)
3. **OCA WebSocket** — requires reverse-engineering encrypted authentication; not pursued

---
[← File Formats](06-file-formats.md) | [Index](README.md) | [OCA Probing Results →](08-oca-probing-results.md)
