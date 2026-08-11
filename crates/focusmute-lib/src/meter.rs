//! Live signal-level metering (`CMD_GET_METER`).
//!
//! Protocol captured from Focusrite Control 2 traffic and confirmed on
//! 2i2 4th Gen hardware: `METER_INFO` returns `[num_meters:u16, magic:u16]`
//! (2i2: `[66, 0x5A0C]`); `GET_METER` takes `[pad:u16=0][count:u16][magic:u32=1]`
//! and returns `count` × u32 values in the 12-bit range 0–4095.
//!
//! 2i2 meter index map: `[0]` = Analogue Input 1, `[1]` = Analogue Input 2
//! (USB capture meters `[2]`/`[3]` mirror them) — the same 0-based input
//! numbering that `MuteStrategy::input_indices` uses.

use crate::device::{Result, ScarlettDevice};
use crate::protocol;

/// Meter slot count on small-format 4th Gen devices (confirmed on 2i2).
pub const METER_COUNT: u16 = 66;

/// Meter values are 12-bit: 0..=4095.
pub const METER_MAX: u32 = 4095;

/// Size of the transact response header: `[status:u32][byte_count:u32]`.
///
/// The out buffer passed to `transact` MUST include it — an undersized
/// buffer makes the SwRoot driver stall ~1.6 s and answer with an error
/// struct whose status code (2) is easily mistaken for meter data
/// (probed on 2i2 hardware).
const RESPONSE_HEADER: usize = 8;

/// Read `count` live meter levels from the device.
pub fn read_meters(device: &impl ScarlettDevice, count: u16) -> Result<Vec<u32>> {
    // Payload: [pad:u16=0][num_meters:u16][magic:u32=1]
    let mut payload = [0u8; 8];
    payload[2..4].copy_from_slice(&count.to_le_bytes());
    payload[4..8].copy_from_slice(&1u32.to_le_bytes());

    let resp = device.transact(
        protocol::CMD_GET_METER,
        &payload,
        RESPONSE_HEADER + count as usize * 4,
    )?;
    if resp.len() <= RESPONSE_HEADER {
        return Err(crate::device::DeviceError::TransactFailed(format!(
            "GET_METER response too short: {} bytes",
            resp.len()
        )));
    }
    Ok(resp[RESPONSE_HEADER..]
        .chunks_exact(4)
        .take(count as usize)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Highest live level among the strategy's input meter indices.
pub fn max_input_level(levels: &[u32], input_indices: &[usize]) -> u32 {
    input_indices
        .iter()
        .filter_map(|&i| levels.get(i).copied())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::mock::MockDevice;

    /// Build a response in the on-wire framing captured from hardware:
    /// `[status:u32=0][byte_count:u32][values…]`.
    fn meter_response(values: &[u32]) -> Vec<u8> {
        let mut resp = Vec::with_capacity(RESPONSE_HEADER + values.len() * 4);
        resp.extend_from_slice(&0u32.to_le_bytes());
        resp.extend_from_slice(&((values.len() * 4) as u32).to_le_bytes());
        resp.extend(values.iter().flat_map(|v| v.to_le_bytes()));
        resp
    }

    #[test]
    fn read_meters_sends_documented_payload_and_header_sized_buffer() {
        let dev = MockDevice::new();
        dev.add_transact_response(protocol::CMD_GET_METER, meter_response(&[0; 66]));

        read_meters(&dev, 66).unwrap();

        let calls = dev.transact_payloads.borrow();
        assert_eq!(calls.len(), 1);
        let (cmd, payload) = &calls[0];
        assert_eq!(*cmd, protocol::CMD_GET_METER);
        // [pad:u16=0][num_meters:u16=66][magic:u32=1]
        assert_eq!(payload, &[0, 0, 66, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn read_meters_skips_transact_header_and_parses_le_u32() {
        let dev = MockDevice::new();
        dev.add_transact_response(protocol::CMD_GET_METER, meter_response(&[4095, 0, 1234]));

        let levels = read_meters(&dev, 3).unwrap();
        assert_eq!(levels, vec![4095, 0, 1234]);
    }

    #[test]
    fn read_meters_tolerates_short_response() {
        let dev = MockDevice::new();
        // Device returns fewer meters than requested — parse what's there.
        dev.add_transact_response(protocol::CMD_GET_METER, meter_response(&[7, 8]));

        let levels = read_meters(&dev, 4).unwrap();
        assert_eq!(levels, vec![7, 8]);
    }

    #[test]
    fn read_meters_rejects_header_only_response() {
        let dev = MockDevice::new();
        dev.add_transact_response(protocol::CMD_GET_METER, meter_response(&[]));

        assert!(read_meters(&dev, 66).is_err());
    }

    #[test]
    fn max_input_level_picks_max_of_configured_inputs() {
        let levels = vec![100, 900, 4095, 0];
        assert_eq!(max_input_level(&levels, &[0, 1]), 900);
        assert_eq!(max_input_level(&levels, &[0]), 100);
        // Out-of-range indices are ignored; empty → 0.
        assert_eq!(max_input_level(&levels, &[9]), 0);
        assert_eq!(max_input_level(&levels, &[]), 0);
    }
}
