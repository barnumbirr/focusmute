# Firmware Binary Analysis — Scarlett 2i2 4th Gen

## File

- **Path:** `scarlett2-1235-8219-2128.bin`
- **Size:** 199,220 bytes (0x30A34)
- **Filename encodes:** `scarlett2-{VID}-{PID}-{version}.bin`

## Outer Header (0x00-0x33, 52 bytes, all big-endian)

Format confirmed from Geoffrey Bennett's `alsa-scarlett-gui` (`scarlett2-firmware.h`):

```c
struct scarlett2_firmware_header {
  char     magic[8];             // "SCARLETT"
  uint16_t usb_vid;              // Big-endian
  uint16_t usb_pid;              // Big-endian
  uint32_t firmware_version;     // Big-endian
  uint32_t firmware_length;      // Big-endian
  uint8_t  sha256[32];           // SHA-256 of payload (offset 0x34 to EOF)
};
```

| Offset | Size | Field | Value |
|--------|------|-------|-------|
| 0x00 | 8 | `magic` | `"SCARLETT"` |
| 0x08 | 2 | `usb_vid` | 0x1235 (Focusrite) |
| 0x0A | 2 | `usb_pid` | 0x8219 (Scarlett 2i2 4th Gen) |
| 0x0C | 4 | `firmware_version` | 2128 (0x00000850) |
| 0x10 | 4 | `firmware_length` | 199,168 (0x00030A00) |
| 0x14 | 32 | `sha256` | SHA-256 of bytes [0x34:EOF] (verified correct) |

## Payload: Encrypted

The payload (offset 0x34 onward) is **fully encrypted**.

- **Entropy:** 7.996 bits/byte (max 8.0) — rules out compression or raw code
- **Strings:** Only `"SCARLETT"` from the plaintext header; no readable text in payload
- **Processor:** XMOS XU216 (XCore200 ISA, not ARM)
- **Encryption:** AES-128, non-ECB mode (CBC or CTR) with per-firmware 8-byte nonce + CMAC-AES-128 authentication
- **Key storage:** Burned into XMOS OTP (one-time-programmable) silicon — never leaves the chip
- **Confirmation:** `device_firmware_schema.json` field `bootAndJTAGMode.bits.isFirmwareEncrypted` (bit 7 at descriptor offset 48)

## Payload Layout (detailed)

| File Offset | Size | Content |
|-------------|------|---------|
| 0x34-0x37 | 4B | Format magic `0x00FF8A7B` (constant across all devices) |
| 0x38-0x3F | 8B | Per-firmware nonce (unique per build) |
| 0x40-0x43 | 4B | Tile count: 2 |
| 0x44-0x53 | 16B | Tile descriptors (config + offset, 8B each) |
| 0x54-0x63 | 16B | Padding/metadata |
| 0x64-0xB7 | 84B | 3 section descriptors (28B each: type + flags + size + 16B CMAC) |
| 0xB8-0xBF | 8B | Descriptor trailer |
| 0xC0-0x857 | 1,944B | **ENCRYPTED** section 1 data |
| 0x858-0x1FFF | 6,056B | **PLAINTEXT** boot stub (identical across all Gen4 devices) |
| 0x2000-0x30963 | ~190KB | **ENCRYPTED** sections 2+3 data |
| 0x30964-end | ~208B | Zero padding (512-byte alignment) |

### Section Descriptors (2i2 4th Gen)

| Section | Type | Flags | Size | CMAC-AES-128 |
|---------|------|-------|------|--------------|
| 1 | 0x0032 | 0x0D | 0x803B (32,827B) | `c2fb78ce 1aa6d93a 35508cc9 6956c6d6` |
| 2 | 0x1004 | 0x01C00040 | 0x601E (24,606B) | `aa5d470a 6fe6eaba af141aec 59d69e9f` |
| 3 | 0x4400 | 0x01400040 | 0x7635 (30,261B) | `699606ed c2f8b1e3 1f6aa31b c8982f4c` |

### Per-Model Nonces

| Device | Nonce (8 bytes) |
|--------|-----------------|
| Gen2 18i20 | `0ea34d83 507bd4db` |
| Gen3 2i2 | `87beafe0 b5e6fc4c` |
| Gen4 Solo | `fc662b5e 734a9b75` |
| Gen4 2i2 | `2a6337e0 5091f285` |
| Gen4 4i4 | `448b4726 3539f8cb` |

### Per-Model Tile Config

The value at payload offset 0x10 (duplicated at 0x18) is an XMOS tile memory configuration register, unique per model:

| Model | PID | Tile Config |
|-------|-----|-------------|
| Solo | 0x8218 | 0x0040B901 |
| 2i2 | 0x8219 | 0x00C0900C |
| 4i4 | 0x821A | 0x00403702 |

## Partially Plaintext Internal Header (0x34-0xC0)

Before the encrypted code, a structured header is visible. Values confirmed constant across multiple Gen4 firmware files (Solo, 2i2, 4i4):

| Payload Offset | File Offset | Size | Field | Value | Constant? |
|----------------|-------------|------|-------|-------|-----------|
| 0x00 | 0x34 | 4 | Format magic | 0x00FF8A7B | All devices |
| 0x04 | 0x38 | 8 | Nonce/IV | varies | Per-device |
| 0x0C | 0x40 | 4 | Tile count | 2 | All devices |
| 0x10 | 0x44 | 4 | Unknown | 0x00C0900C (2i2) | Per-device |
| 0x14 | 0x48 | 4 | Descriptor table end | 0x80 | All devices |
| 0x18 | 0x4C | 4 | Unknown (= field at 0x10) | 0x00C0900C (2i2) | Per-device |
| 0x1C | 0x50 | 4 | Encrypted data start | 0xC0 (Gen4) / 0x80 (Gen3) | Per-gen |
| 0x20 | 0x54 | 12 | Padding | zeros | All devices |

## Section Descriptor Table (0x64-0xB7)

Three 28-byte entries, each containing type + flags + size + 16-byte CMAC hash (matching the "Section Descriptors" table above):

| Entry | Type | Flags | Size | CMAC (16B) | Notes |
|-------|------|-------|------|------------|-------|
| 0 | 0x0032 | 0x0D | 0x803B | c2fb78ce... | Boot/section 1 |
| 1 | 0x1004 | 0x01C00040 | 0x601E | aa5d470a... | Tile 0 code segment |
| 2 | 0x4400 | 0x01400040 | 0x7635 | 699606ed... | Tile 1 code segment |

**XMOS load addresses (constant across all Gen4):**
- Tile 0: `0x01C00040`
- Tile 1: `0x01400040`

The 16-byte hashes are integrity checks of the decrypted segment content (unverifiable without the decryption key).

## File Tail and Alignment

- **Last non-zero byte:** file offset 0x30963
- **Zero padding:** 208 bytes (0xD0) to bring payload to 512-byte boundary
- **Payload size** 199,168 is 512-byte aligned (flash write granularity)

## Firmware Update Flow

From `alsa-scarlett-gui` source (`device-update-firmware.c`):

1. Parse `.bin` header, verify SHA-256 of payload
2. Erase `App_Settings` flash segment (resets config)
3. Erase `App_Upgrade` flash segment
4. Write payload to `App_Upgrade` in chunks (max 1024 bytes each) via `snd_hwdep_write()`
5. Reboot device

The payload is written as-is; the XMOS bootloader handles decryption on boot.

## Decryption Attempt — All Software Vectors Exhausted

### Vectors investigated

| Vector | Result |
|--------|--------|
| Windows driver (`FocusriteUsbSwRoot.sys`, 122KB) | No AES S-box, no crypto imports, no key material — pure USB pass-through |
| Linux driver / `alsa-scarlett-gui` | Only validates SHA-256, writes blob as-is to device |
| Focusrite Control 2 (`Focusrite Control 2.exe`, 78MB) | Contains firmware ZIPs all marked `"encrypted": true`, no decryption logic |
| ECB mode weakness | Ruled out — zero repeating 16-byte blocks across ~12k encrypted blocks |
| Known-plaintext (cross-device XOR) | Different nonces per firmware produce unrelated ciphertext; XOR yields noise (4.75 bits/byte) |
| Gen2 firmware | Fully plaintext (entropy 5.3 bits/byte), but different architecture/keys |
| Gen3 firmware | Fully encrypted (entropy 7.33 bits/byte) |
| XMOS XU216 CVEs | None published |

### Why it's not feasible

- AES-128 key is burned into XMOS OTP silicon — never exposed to any host software
- Non-ECB mode with per-firmware nonces makes cross-firmware analysis useless
- CMAC-AES-128 authentication prevents ciphertext modification
- No software on any platform (Windows/Linux/macOS) ever sees the decryption key
- JTAG can be (and likely is) permanently disabled via OTP

### Remaining vectors (physical only)

- Voltage/clock glitching to bypass OTP read protection during boot
- Chip decapping + focused ion beam (FIB) probing of OTP cells
- Side-channel analysis (power/EM emanation) during boot-time decryption

## Implications

The firmware is fully encrypted — we cannot extract:
- LED count or layout for different models
- Descriptor structure definitions
- Metering gradient palette sizes
- Any model-specific protocol constants

To discover these values for non-2i2 models, the only viable approach is **live device probing** via the descriptor protocol.

## Addendum: Scarlett4 Firmware Container Format (Big 4th Gen)

> Source: [alsa-scarlett-gui `scarlett4-firmware.h`](https://github.com/geoffreybennett/alsa-scarlett-gui) (Feb 2026).

The big 4th Gen models (16i16, 18i16, 18i20) use a new multi-section firmware container format distinct from the single-image format used by the small 4th Gen (Solo, 2i2, 4i4) and earlier generations.

### Container Structure

**Magic strings**:
| Magic | Meaning |
|-------|---------|
| `SCARLBOX` | Outer container holding 1-3 firmware sections |
| `SCARLET4` | Main XMOS application firmware |
| `SCARLESP` | ESP32 WiFi/Bluetooth chip firmware |
| `SCARLEAP` | Leapfrog transitional bootloader (for multi-step upgrade) |

**Container header** (big-endian):
```
offset 0:  u16 BE  usb_vid
offset 2:  u16 BE  usb_pid
offset 4:  u32 BE  firmware_version[4]  (4-valued version, e.g. 1.2.3.4)
offset 20: u32 BE  num_sections         (1-3)
```

**Per-section header** (big-endian):
```
offset 0:  u16 BE  usb_vid
offset 2:  u16 BE  usb_pid
offset 4:  u32 BE  firmware_version[4]
offset 20: u32 BE  firmware_length
offset 24: u8[32]  sha256               (SHA-256 hash of firmware data)
```

**Key differences from small 4th Gen firmware**:
- 4-valued version numbers (instead of single integer)
- SHA-256 integrity verification (ESP firmware also uses MD5)
- Multi-section containers (vs single encrypted XMOS image)
- Big-endian headers (vs little-endian in the XMOS boot format)
- Firmware stored in `/usr/lib/firmware/scarlett4/` (separate repo: [scarlett4-firmware](https://github.com/geoffreybennett/scarlett4-firmware))

### ESP32 Firmware Update Process

The big models have a dual-processor architecture (XMOS + ESP32). Firmware upgrades follow a multi-step "leapfrog" process:

1. Check if ESP firmware needs updating (compare device version with container)
2. If ESP update needed and device is NOT running leapfrog bootloader:
   - Erase current app firmware
   - Upload leapfrog bootloader
   - Reboot device (comes back in leapfrog mode)
3. Upload ESP firmware (no reboot)
4. Erase leapfrog, upload final app firmware, reboot

This explains the `FCP_SOCKET_ERR_NOT_LEAPFROG` error code in the socket protocol — the server rejects ESP updates if the device isn't in leapfrog mode first.

### Relevance to Our 2i2

The 2i2 (and other small 4th Gen) uses the older single-image format analyzed in the sections above. The Scarlett4 container format only applies to the big models. However, the existence of dual-processor firmware confirms that the big models have significantly different internals, which affects multi-model support plans (see [16-multi-model-mute-design.md](16-multi-model-mute-design.md)).

## References

- [Geoffrey Bennett scarlett2 firmware utility](https://github.com/geoffreybennett/scarlett2)
- [Geoffrey Bennett scarlett2-firmware repository](https://github.com/geoffreybennett/scarlett2-firmware)
- [Geoffrey Bennett scarlett4-firmware repository](https://github.com/geoffreybennett/scarlett4-firmware) (big 4th Gen)
- [Geoffrey Bennett alsa-scarlett-gui](https://github.com/geoffreybennett/alsa-scarlett-gui)
- [XMOS Safeguard IP documentation (XTC Tools v15.3)](https://www.xmos.com/documentation/XM-014363-PC/html/tools-guide/tutorials/safeguard-ip/safeguard.html)
- [XMOS XBURN Command-Line Manual](https://www.xmos.com/documentation/XM-014363-PC/html/tools-guide/tools-ref/cmd-line-tools/xburn-manual/xburn-manual.html)
- [Focusrite Scarlett hardware analysis by fenugrec](http://www.qcte.ca/audio/focusrite_scarlett/)

---
[← Protocol Reference](13-protocol-reference.md) | [Index](README.md) | [Build & Packaging →](15-build-and-packaging.md)
