//! Device offsets — model-specific descriptor offsets and LED counts.
//!
//! Encapsulates the per-model values needed for LED control: descriptor
//! offsets and LED array sizes. Constructed from the firmware schema when
//! available, or defaults to Scarlett 2i2 4th Gen hardcoded values from
//! `protocol.rs`.

use crate::protocol;
use crate::schema::SchemaConstants;

/// Descriptor offsets and LED counts for a specific device model.
///
/// These values are either extracted from the firmware schema or
/// fall back to hardcoded defaults (Scarlett 2i2 4th Gen).
#[derive(Debug, Clone)]
pub struct DeviceOffsets {
    /// Offset of `enableDirectLEDMode` (u8). Not in schema — believed constant across Gen 4.
    pub enable_direct_led: u32,
    /// Offset of `directLEDValues` (u32 array).
    pub direct_led_values: u32,
    /// Number of `directLEDValues` entries.
    pub direct_led_count: usize,
    /// DATA_NOTIFY event ID for `directLEDValues` changes. Not in schema.
    pub direct_led_notify: u32,
}

impl DeviceOffsets {
    /// Create offsets from firmware schema constants.
    ///
    /// Uses schema-extracted values for LED counts and offsets, with
    /// hardcoded fallbacks for values not present in the schema
    /// (`enable_direct_led`, `direct_led_notify`).
    pub fn from_schema(sc: &SchemaConstants) -> Self {
        Self {
            enable_direct_led: protocol::OFF_ENABLE_DIRECT_LED,
            direct_led_values: sc.direct_led_offset,
            direct_led_count: sc.direct_led_count,
            direct_led_notify: protocol::NOTIFY_DIRECT_LED_VALUES,
        }
    }

    /// Size of `directLEDValues` in bytes (count * 4).
    pub fn direct_led_size(&self) -> u32 {
        (self.direct_led_count * 4) as u32
    }
}

impl Default for DeviceOffsets {
    /// Default offsets matching the Scarlett 2i2 4th Gen (from `protocol.rs` constants).
    fn default() -> Self {
        Self {
            enable_direct_led: protocol::OFF_ENABLE_DIRECT_LED,
            direct_led_values: protocol::OFF_DIRECT_LED_VALUES,
            direct_led_count: protocol::DIRECT_LED_COUNT,
            direct_led_notify: protocol::NOTIFY_DIRECT_LED_VALUES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_protocol_constants() {
        let offsets = DeviceOffsets::default();
        assert_eq!(offsets.enable_direct_led, protocol::OFF_ENABLE_DIRECT_LED);
        assert_eq!(offsets.direct_led_values, protocol::OFF_DIRECT_LED_VALUES);
        assert_eq!(offsets.direct_led_count, protocol::DIRECT_LED_COUNT);
        assert_eq!(
            offsets.direct_led_notify,
            protocol::NOTIFY_DIRECT_LED_VALUES
        );
    }

    #[test]
    fn from_schema_uses_schema_values() {
        let sc = SchemaConstants {
            product_name: "Scarlett 4i4 4th Gen".into(),
            max_leds: 56,
            max_inputs: 4,
            max_outputs: 4,
            gradient_count: 15,
            gradient_offset: 500,
            gradient_notify: 12,
            direct_led_count: 56,
            direct_led_offset: 100,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let offsets = DeviceOffsets::from_schema(&sc);
        assert_eq!(offsets.direct_led_values, 100);
        assert_eq!(offsets.direct_led_count, 56);
        // These come from protocol constants, not schema
        assert_eq!(offsets.enable_direct_led, protocol::OFF_ENABLE_DIRECT_LED);
        assert_eq!(
            offsets.direct_led_notify,
            protocol::NOTIFY_DIRECT_LED_VALUES
        );
    }

    #[test]
    fn from_schema_2i2_matches_default() {
        let sc = SchemaConstants {
            product_name: "Scarlett 2i2 4th Gen".into(),
            max_leds: 40,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 40,
            direct_led_offset: 92,
            metering_segments: 25,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: "2.0.2417.0".into(),
        };
        let from_schema = DeviceOffsets::from_schema(&sc);
        let default = DeviceOffsets::default();
        assert_eq!(from_schema.enable_direct_led, default.enable_direct_led);
        assert_eq!(from_schema.direct_led_values, default.direct_led_values);
        assert_eq!(from_schema.direct_led_count, default.direct_led_count);
        assert_eq!(from_schema.direct_led_notify, default.direct_led_notify);
    }

    #[test]
    fn direct_led_size_computed_correctly() {
        let offsets = DeviceOffsets::default();
        assert_eq!(
            offsets.direct_led_size(),
            (protocol::DIRECT_LED_COUNT * 4) as u32
        );

        let custom = DeviceOffsets {
            direct_led_count: 56,
            ..Default::default()
        };
        assert_eq!(custom.direct_led_size(), 224); // 56 * 4
    }

    #[test]
    fn clone_produces_equal_offsets() {
        let offsets = DeviceOffsets::from_schema(&SchemaConstants {
            product_name: "Test".into(),
            max_leds: 56,
            max_inputs: 4,
            max_outputs: 4,
            gradient_count: 15,
            gradient_offset: 500,
            gradient_notify: 12,
            direct_led_count: 56,
            direct_led_offset: 100,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        });
        let cloned = offsets.clone();
        assert_eq!(cloned.enable_direct_led, offsets.enable_direct_led);
        assert_eq!(cloned.direct_led_values, offsets.direct_led_values);
        assert_eq!(cloned.direct_led_count, offsets.direct_led_count);
        assert_eq!(cloned.direct_led_notify, offsets.direct_led_notify);
    }

    #[test]
    fn debug_format_contains_field_names() {
        let offsets = DeviceOffsets::default();
        let debug = format!("{offsets:?}");
        assert!(debug.contains("enable_direct_led"));
        assert!(debug.contains("direct_led_count"));
    }
}
