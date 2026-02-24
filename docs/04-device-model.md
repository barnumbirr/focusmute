# Device Model - Scarlett 2i2 4th Gen

## Device Identity

| Property | Value |
|----------|-------|
| Product Name | Scarlett 2i2 4th Gen |
| Product ID | 33305 (0x8219) |
| Firmware | 2.0.2417.0 |
| Driver | 4.143.0.261 |

## Supported Devices (from embedded assets)

The binary contains PNG assets and firmware bundles for:
- **Scarlett 2i2 4th Gen** (your device)
- **Scarlett 4i4 4th Gen**
- **Scarlett 18i20 4th Gen**
- **Scarlett 2i2 FF (Focusrite Fast) 4th Gen** (variant)

Legacy Scarlett MixControl supports: 6i6, 8i6, 18i6, 18i8, 18i20 (older generations)

## Input Channels

### Preamps (inputId 128-129)
| Channel | ID | Type | Gain Range | Features |
|---------|-----|------|-----------|----------|
| Analogue 1 | 128 | preamp | 0-70 dB | Auto Gain, Air, Phantom, Line/Inst |
| Analogue 2 | 129 | preamp | 0-70 dB | Auto Gain, Air, Phantom, Line/Inst |

### Post-DSP Inputs (inputId 772-773)
- Post-DSP version of Analogue 1 (772)
- Post-DSP version of Analogue 2 (773)

### Playback Channels (inputId 1536-1537)
- Playback 1 (1536) - USB/DAW output to device
- Playback 2 (1537) - USB/DAW output to device

## Output Channels

### Analogue Outputs
| Channel | ID | Type |
|---------|-----|------|
| Output 1 | 128 | line |
| Output 2 | 129 | line |

### Digital Outputs
| Channel | ID | Type |
|---------|-----|------|
| Loopback 1 | 1538 | host |
| Loopback 2 | 1539 | host |

## Mixer Sources (for routing)

| Source ID | Description |
|-----------|-------------|
| 768 | Mix output left (e.g., Direct Monitor L) |
| 769 | Mix output right (e.g., Direct Monitor R) |
| 770 | Loopback mix left |
| 771 | Loopback mix right |
| 772 | Post-DSP Analogue 1 |
| 773 | Post-DSP Analogue 2 |
| 1536 | Playback 1 |
| 1537 | Playback 2 |

## Input Controls Per Channel (All Scarlett Models)

> **Note**: Not all controls are available on the 2i2. This list represents all controls found in the FC2 binary across all supported device models. The 2i2 supports: `phantomPower`, `air`, `airMode`, `clipSafe`, `mode` (line/inst), `preampGain`, `linked`.

| Control | Type | Description |
|---------|------|-------------|
| `phantomPower` | bool | 48V phantom power |
| `air` | bool | Air mode on/off |
| `airMode` | enum | presence, presence+drive |
| `clipSafe` | bool | Clip Safe auto-attenuation |
| `mode` | enum | line, inst |
| `preampGain` | dB | 0.0 - 70.0 dB |
| `linked` | bool | Stereo link channels |
| `insert` | bool | Hardware insert |
| `highPassFilter` | bool | High-pass filter |
| `impedance` | enum | Impedance mode |
| `drive` | bool | Drive effect |
| `console` | bool+amount | Console emulation |

## Mixer Capabilities

- **Mixes**: 2 (Direct Monitor, Loopback)
- **Per-channel controls**: level (dB), pan (-1.0 to 1.0), mute, solo
- **Channel groups**: Analogue, Playback
- **Features**: Split to mono, channel hiding, custom names

## Device Features Matrix

From binary analysis, the app checks for these device capabilities:
- Direct Monitor (supported on 2i2)
- Loopback Direct Monitor Mirroring
- Monitor Switching (speaker A/B)
- MSD Mode (Mass Storage Device mode)
- Talkback
- Digital IO Modes (S/PDIF, ADAT)
- LED Brightness adjustment
- LED Sleep
- Phantom Power Persist (across power cycles)
- Video Call Mode
- Main Output: Dim, Level, Mono, Mute
- Multi-Channel Auto Gain
- Firmware Encryption

---
[← AES70/OCA Protocol](03-protocol-aes70.md) | [Index](README.md) | [Actions Catalog →](05-actions-catalog.md)
