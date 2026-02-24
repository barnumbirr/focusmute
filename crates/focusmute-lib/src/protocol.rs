//! Protocol constants for Focusrite Scarlett 4th Gen devices.
//!
//! All values decoded from USB captures and the Linux scarlett2 driver.
//!
//! ## Cross-model compatibility
//!
//! The following are believed to be universal across all 4th Gen models:
//! - IOCTL codes, TRANSACT command codes, DATA_NOTIFY mechanism
//! - Protocol framing (TRANSACT with session token)
//! - Color format (`0xRRGGBB00`)
//! - Descriptor offsets: `enableDirectLEDMode`, `directLEDValues`
//!
//! The following are **confirmed only on the Scarlett 2i2 4th Gen** and may
//! differ on Solo, 4i4, 8i6, 16i16, and 18i20:
//! - `DIRECT_LED_COUNT` (40) — other models likely have different LED counts
//! - `DESCRIPTOR_SIZE` (720) — total descriptor size
//! - LED index-to-physical mapping (see `OFF_DIRECT_LED_VALUES` doc comment)

// ── IOCTL codes ──

/// Synchronous init handshake — must be sent before any TRANSACT.
pub const IOCTL_INIT: u32 = 0x0022_2000;

/// Asynchronous command transport — wraps all USB commands.
pub const IOCTL_TRANSACT: u32 = 0x0022_2008;

/// Notification / interrupt — pends until device fires an interrupt.
/// Input: 0 bytes. Output: 16 bytes (notification bitmask).
/// Classic pending-IRP pattern: submit, pends in driver, completes on device interrupt.
pub const IOCTL_NOTIFY: u32 = 0x0022_200C;

/// Probe IOCTL — raw IOCTL observed in FC2 captures, ~16-byte response.
pub const IOCTL_PROBE: u32 = 0x0022_2004;

// ── TRANSACT command codes (SwRoot byte-swapped from USB) ──

/// USB init (must follow IOCTL_INIT).
pub const CMD_USB_INIT: u32 = 0x0001_0400;

/// Get device configuration — returns session token in bytes 8..16.
pub const CMD_GET_CONFIG: u32 = 0x0004_0400;

/// Read descriptor memory: payload = `[offset:u32, size:u32]`.
pub const CMD_GET_DESCR: u32 = 0x0000_0800;

/// Write descriptor memory: payload = `[offset:u32, size:u32, data...]`.
pub const CMD_SET_DESCR: u32 = 0x0001_0800;

/// Data notification — tells firmware to act on descriptor changes.
/// Payload = `[event_id:u32]`.
pub const CMD_DATA_NOTIFY: u32 = 0x0002_0800;

/// INFO_DEVMAP — get schema metadata (total size).
/// SwRoot mapping of raw USB command 0x0080000C.
pub const CMD_INFO_DEVMAP: u32 = 0x000C_0800;

/// GET_DEVMAP — get schema data page (1024 bytes per page).
/// SwRoot mapping of raw USB command 0x0080000D.
pub const CMD_GET_DEVMAP: u32 = 0x000D_0800;

/// Meter topology info — returns meter count/config.
/// SwRoot mapping of raw USB command 0x00001000.
pub const CMD_METER_INFO: u32 = 0x0000_0001;

/// Read live meter levels — returns `num_meters` u32 values.
/// SwRoot mapping of raw USB command 0x00001001.
/// Payload: `[pad:u16=0][num_meters:u16][magic:u32=1]`.
pub const CMD_GET_METER: u32 = 0x0001_0001;

// ── Investigation commands (read-only, from FC2 captures) ──

/// Init step 2 (SwRoot) — 96-byte response with firmware info.
pub const CMD_INIT_2: u32 = 0x0002_0000;

/// Mixer topology info — 16-byte response.
pub const CMD_MIX_INFO: u32 = 0x0000_0002;

/// Mux topology info — 20-byte response.
pub const CMD_MUX_INFO: u32 = 0x0000_0003;

/// Flash info — 24-byte response (flash_size, segment_count, reserved).
pub const CMD_INFO_FLASH: u32 = 0x0000_0004;

/// Segment info — 32-byte response. Payload: `[seg:u32]`.
pub const CMD_INFO_SEGMENT: u32 = 0x0001_0004;

/// Read segment data — variable response. Payload: `[seg:u32][off:u32][len:u32]`.
pub const CMD_READ_SEGMENT: u32 = 0x0005_0004;

/// Get mux routing table — 56-byte response. Payload: `[pad:u16=0][table:u16]`.
pub const CMD_GET_MUX: u32 = 0x0001_0003;

/// Sync status — 12-byte response.
pub const CMD_GET_SYNC: u32 = 0x0004_0006;

/// Clock info 2 — 16-byte response.
pub const CMD_CLOCK_2: u32 = 0x0002_0006;

/// Clock info 5 — 12-byte response.
pub const CMD_CLOCK_5: u32 = 0x0005_0006;

/// Driver info — 228-byte response (SwRoot-internal).
pub const CMD_DRIVER_INFO: u32 = 0x0012_0401;

/// Size of each devmap page payload (bytes).
pub const DEVMAP_PAGE_SIZE: usize = 1024;

/// Expected response size for devmap page (8-byte header + payload).
pub const DEVMAP_RESPONSE_SIZE: usize = 8 + DEVMAP_PAGE_SIZE;

// ── Raw USB command codes (Linux direct USB) ──

/// Init step 1 — reset sequence counter.
pub const USB_CMD_INIT_1: u32 = 0x0000_0000;

/// Init step 2 — returns 84 bytes with firmware version at bytes 8-11.
pub const USB_CMD_INIT_2: u32 = 0x0000_0002;

/// Read descriptor memory: payload = `[offset:u32, size:u32]`.
pub const USB_CMD_GET_DATA: u32 = 0x0080_0000;

/// Write descriptor memory: payload = `[offset:u32, size:u32, data...]`.
pub const USB_CMD_SET_DATA: u32 = 0x0080_0001;

/// Data notification / activate — tells firmware to act on descriptor changes.
pub const USB_CMD_DATA_CMD: u32 = 0x0080_0002;

/// Meter topology info (raw USB).
pub const USB_CMD_METER_INFO: u32 = 0x0000_1000;

/// Read live meter levels (raw USB).
pub const USB_CMD_GET_METER: u32 = 0x0000_1001;

/// INFO_DEVMAP — get schema metadata (total size).
pub const USB_CMD_INFO_DEVMAP: u32 = 0x0080_000C;

/// GET_DEVMAP — get schema data page (1024 bytes per page).
pub const USB_CMD_GET_DEVMAP: u32 = 0x0080_000D;

// ── USB control transfer parameters ──

/// `bRequest` for init step 0 (read 24 bytes).
pub const USB_BREQUEST_INIT: u8 = 0;

/// `bRequest` for sending commands (TX).
pub const USB_BREQUEST_TX: u8 = 2;

/// `bRequest` for receiving responses (RX).
pub const USB_BREQUEST_RX: u8 = 3;

/// Timeout per USB control transfer in milliseconds.
pub const USB_TIMEOUT_MS: u64 = 1000;

/// Maximum retries on `-EPROTO` errors.
pub const USB_MAX_RETRIES: usize = 5;

/// USB packet header size (cmd + size + seq + error + pad).
pub const USB_HEADER_SIZE: usize = 16;

// ── Focusrite USB identifiers ──

/// Focusrite vendor ID.
pub const FOCUSRITE_VID: u16 = 0x1235;

/// Device interface GUID registered by FocusriteUsbSwRoot.sys.
/// Used on Windows to enumerate Focusrite device interfaces via SetupDi.
#[cfg(windows)]
pub const FOCUSRITE_GUID: windows::core::GUID = windows::core::GUID {
    data1: 0xAC4D0455,
    data2: 0x50D7,
    data3: 0x4498,
    data4: [0xB3, 0xCD, 0x9A, 0x41, 0xD1, 0x30, 0xB7, 0x59],
};

/// Map a SwRoot command code to its raw USB equivalent.
///
/// Covers the descriptor commands used by `ScarlettDevice` trait methods:
/// - `CMD_GET_DESCR` (0x0000_0800) → `USB_CMD_GET_DATA` (0x0080_0000)
/// - `CMD_SET_DESCR` (0x0001_0800) → `USB_CMD_SET_DATA` (0x0080_0001)
/// - `CMD_DATA_NOTIFY` (0x0002_0800) → `USB_CMD_DATA_CMD` (0x0080_0002)
/// - `CMD_INFO_DEVMAP` (0x000C_0800) → `USB_CMD_INFO_DEVMAP` (0x0080_000C)
/// - `CMD_GET_DEVMAP` (0x000D_0800) → `USB_CMD_GET_DEVMAP` (0x0080_000D)
///
/// Returns `None` for unrecognised commands (e.g. `CMD_USB_INIT`, `CMD_GET_CONFIG`
/// which are SwRoot-only and have no direct USB equivalent).
pub fn swroot_to_usb_cmd(swroot_cmd: u32) -> Option<u32> {
    match swroot_cmd {
        CMD_GET_DESCR => Some(USB_CMD_GET_DATA),
        CMD_SET_DESCR => Some(USB_CMD_SET_DATA),
        CMD_DATA_NOTIFY => Some(USB_CMD_DATA_CMD),
        CMD_INFO_DEVMAP => Some(USB_CMD_INFO_DEVMAP),
        CMD_GET_DEVMAP => Some(USB_CMD_GET_DEVMAP),
        CMD_METER_INFO => Some(USB_CMD_METER_INFO),
        CMD_GET_METER => Some(USB_CMD_GET_METER),
        _ => None,
    }
}

// ── Descriptor offsets (Scarlett 4th Gen, shared across models) ──

/// `enableDirectLEDMode` — u8 at this offset. 0=normal, 2=halo override.
pub const OFF_ENABLE_DIRECT_LED: u32 = 77;

/// `directLEDColour` — u32 color for single-LED update via DATA_NOTIFY(8).
/// Must be written before `directLEDIndex`. Format: `0xRRGGBB00`.
pub const OFF_DIRECT_LED_COLOUR: u32 = 84;

/// `directLEDIndex` — u8 LED index (0-39) for single-LED update via DATA_NOTIFY(8).
/// Must be written after `directLEDColour`.
pub const OFF_DIRECT_LED_INDEX: u32 = 88;

/// `selectedInput` — u8 at this offset. 0=Input 1, 1=Input 2.
/// Indicates which input is currently selected via the front-panel Select button.
/// Schema: notify-device=17, set-via-parameter-buffer=true.
/// WARNING: Do NOT write + DATA_NOTIFY(17) — crashes the device (see doc 10).
pub const OFF_SELECTED_INPUT: u32 = 331;

/// `directLEDValues[40]` — u32 array (160 bytes) starting at this offset.
/// Each entry is a color in `0xRRGGBB00` format.
///
/// Scarlett 2i2 4th Gen LED index map:
///   0     = Input 1 "1" number indicator
///   1-7   = Input 1 halo ring (7 segments)
///   8     = Input 2 "2" number indicator
///   9-15  = Input 2 halo ring (7 segments)
///   16-26 = Output halo ring (11 segments)
///   27    = Select button LED 1
///   28    = Inst button
///   29    = 48V button
///   30    = Air button
///   31    = Auto button
///   32    = Safe button
///   33-34 = Direct button (2 LEDs)
///   35    = Select button (2nd LED)
///   36    = Direct button crossed rings
///   37-38 = Output indicator (2 LEDs)
///   39    = USB symbol
pub const OFF_DIRECT_LED_VALUES: u32 = 92;

/// Number of directLEDValues entries (confirmed on 2i2; may differ on other models).
pub const DIRECT_LED_COUNT: usize = 40;

/// Size of directLEDValues in bytes (40 * 4).
pub const DIRECT_LED_SIZE: u32 = (DIRECT_LED_COUNT * 4) as u32;

/// `parameterValue` — u8 at this offset. Used by parameter-buffer mechanism.
pub const OFF_PARAMETER_VALUE: u32 = 252;

/// `parameterChannel` — u8 at this offset. FCP message type for parameter-buffer writes.
pub const OFF_PARAMETER_CHANNEL: u32 = 253;

/// `inputTRSPresent` — u8[2] starting at this offset. Per-channel jack detection.
/// 1 = cable detected, 0 = no cable.
/// Notification: IOCTL_NOTIFY bit 0x20000000 (FCP_NOTIFY_TRS_INPUT_CHANGE).
/// Despite the name, combo jacks (XLR/TRS) likely report any insertion type.
pub const OFF_INPUT_TRS_PRESENT: u32 = 345;

/// Number of input TRS detection channels (2 on 2i2).
pub const INPUT_TRS_COUNT: usize = 2;

/// `brightness` — eBrightnessMode (u8) at this offset. 0=High, 1=Medium, 2=Low.
pub const OFF_BRIGHTNESS: u32 = 711;

// ── DATA_NOTIFY event IDs ──

/// Notify after writing `directLEDValues`.
pub const NOTIFY_DIRECT_LED_VALUES: u32 = 5;

/// Notify after writing `directLEDColour` / `directLEDIndex`.
pub const NOTIFY_DIRECT_LED_COLOUR: u32 = 8;

/// Notify after writing brightness.
pub const NOTIFY_BRIGHTNESS: u32 = 37;

// ── Descriptor total size ──

/// Full descriptor size for a bulk read (confirmed on 2i2; may differ on other models).
pub const DESCRIPTOR_SIZE: u32 = 720;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_codes_distinct() {
        let cmds = [
            CMD_USB_INIT,
            CMD_GET_CONFIG,
            CMD_GET_DESCR,
            CMD_SET_DESCR,
            CMD_DATA_NOTIFY,
            CMD_INFO_DEVMAP,
            CMD_GET_DEVMAP,
            CMD_METER_INFO,
            CMD_GET_METER,
            CMD_INIT_2,
            CMD_MIX_INFO,
            CMD_MUX_INFO,
            CMD_INFO_FLASH,
            CMD_INFO_SEGMENT,
            CMD_READ_SEGMENT,
            CMD_GET_MUX,
            CMD_GET_SYNC,
            CMD_CLOCK_2,
            CMD_CLOCK_5,
            CMD_DRIVER_INFO,
        ];
        for i in 0..cmds.len() {
            for j in (i + 1)..cmds.len() {
                assert_ne!(cmds[i], cmds[j], "commands at index {i} and {j} collide");
            }
        }
    }

    #[test]
    fn notify_events_distinct() {
        let events = [
            NOTIFY_DIRECT_LED_VALUES,
            NOTIFY_DIRECT_LED_COLOUR,
            NOTIFY_BRIGHTNESS,
        ];
        for i in 0..events.len() {
            for j in (i + 1)..events.len() {
                assert_ne!(
                    events[i], events[j],
                    "notify events at index {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn ioctl_codes_distinct() {
        let ioctls = [IOCTL_INIT, IOCTL_TRANSACT, IOCTL_NOTIFY, IOCTL_PROBE];
        for i in 0..ioctls.len() {
            for j in (i + 1)..ioctls.len() {
                assert_ne!(
                    ioctls[i], ioctls[j],
                    "IOCTL codes at index {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn descriptor_offsets_no_overlap() {
        // enableDirectLEDMode (1 byte at 77) should not overlap directLEDValues (160 bytes at 92)
        const { assert!(OFF_ENABLE_DIRECT_LED < OFF_DIRECT_LED_VALUES) };
        // directLEDValues (160 bytes at 92) should not overlap parameterValue (1 byte at 252)
        const { assert!(OFF_DIRECT_LED_VALUES + DIRECT_LED_SIZE <= OFF_PARAMETER_VALUE) };
        // parameterValue (1 byte at 252) should not overlap parameterChannel (1 byte at 253)
        const { assert!(OFF_PARAMETER_VALUE < OFF_PARAMETER_CHANNEL) };
        // selectedInput (1 byte at 331) should not overlap inputTRSPresent (2 bytes at 345)
        const { assert!(OFF_SELECTED_INPUT < OFF_INPUT_TRS_PRESENT) };
        // brightness (1 byte at 711) should fit within descriptor
        const { assert!(OFF_BRIGHTNESS < DESCRIPTOR_SIZE) };
    }

    #[test]
    fn direct_led_size_consistent() {
        assert_eq!(DIRECT_LED_SIZE, (DIRECT_LED_COUNT * 4) as u32);
        assert_eq!(DIRECT_LED_COUNT, 40);
    }

    #[test]
    fn devmap_response_size_consistent() {
        assert_eq!(DEVMAP_RESPONSE_SIZE, 8 + DEVMAP_PAGE_SIZE);
        assert_eq!(DEVMAP_PAGE_SIZE, 1024);
    }

    #[test]
    fn devmap_commands_distinct_from_descriptor_commands() {
        assert_ne!(CMD_INFO_DEVMAP, CMD_GET_DESCR);
        assert_ne!(CMD_INFO_DEVMAP, CMD_SET_DESCR);
        assert_ne!(CMD_GET_DEVMAP, CMD_GET_DESCR);
        assert_ne!(CMD_GET_DEVMAP, CMD_SET_DESCR);
        assert_ne!(CMD_INFO_DEVMAP, CMD_GET_DEVMAP);
    }

    // ── Raw USB constants ──

    #[test]
    fn usb_command_codes_distinct() {
        let cmds = [
            USB_CMD_INIT_1,
            USB_CMD_INIT_2,
            USB_CMD_GET_DATA,
            USB_CMD_SET_DATA,
            USB_CMD_DATA_CMD,
            USB_CMD_INFO_DEVMAP,
            USB_CMD_GET_DEVMAP,
            USB_CMD_METER_INFO,
            USB_CMD_GET_METER,
        ];
        for i in 0..cmds.len() {
            for j in (i + 1)..cmds.len() {
                assert_ne!(
                    cmds[i], cmds[j],
                    "USB commands at index {i} and {j} collide"
                );
            }
        }
    }

    #[test]
    fn swroot_to_usb_maps_all_descriptor_commands() {
        assert_eq!(swroot_to_usb_cmd(CMD_GET_DESCR), Some(USB_CMD_GET_DATA));
        assert_eq!(swroot_to_usb_cmd(CMD_SET_DESCR), Some(USB_CMD_SET_DATA));
        assert_eq!(swroot_to_usb_cmd(CMD_DATA_NOTIFY), Some(USB_CMD_DATA_CMD));
        assert_eq!(
            swroot_to_usb_cmd(CMD_INFO_DEVMAP),
            Some(USB_CMD_INFO_DEVMAP)
        );
        assert_eq!(swroot_to_usb_cmd(CMD_GET_DEVMAP), Some(USB_CMD_GET_DEVMAP));
    }

    #[test]
    fn swroot_to_usb_maps_meter_commands() {
        assert_eq!(swroot_to_usb_cmd(CMD_METER_INFO), Some(USB_CMD_METER_INFO));
        assert_eq!(swroot_to_usb_cmd(CMD_GET_METER), Some(USB_CMD_GET_METER));
    }

    #[test]
    fn swroot_to_usb_returns_none_for_swroot_only() {
        // CMD_USB_INIT and CMD_GET_CONFIG are SwRoot-only
        assert_eq!(swroot_to_usb_cmd(CMD_USB_INIT), None);
        assert_eq!(swroot_to_usb_cmd(CMD_GET_CONFIG), None);
        // Random unknown command
        assert_eq!(swroot_to_usb_cmd(0xDEADBEEF), None);
    }

    #[test]
    fn usb_brequest_values_distinct() {
        assert_ne!(USB_BREQUEST_INIT, USB_BREQUEST_TX);
        assert_ne!(USB_BREQUEST_INIT, USB_BREQUEST_RX);
        assert_ne!(USB_BREQUEST_TX, USB_BREQUEST_RX);
    }

    #[test]
    fn usb_header_size_is_16() {
        // cmd(4) + size(2) + seq(2) + error(4) + pad(4) = 16
        assert_eq!(USB_HEADER_SIZE, 16);
    }
}
