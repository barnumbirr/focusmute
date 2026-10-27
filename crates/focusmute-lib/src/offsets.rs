//! Device offsets — model-specific descriptor offsets and LED counts.
//!
//! Encapsulates the per-model values needed for LED control: descriptor
//! offsets and LED array sizes. Constructed from the firmware schema when
//! available, or defaults to Scarlett 2i2 4th Gen hardcoded values from
//! `protocol.rs`.

use crate::protocol;
use crate::schema::{PreampGain, SchemaConstants};

/// Descriptor offsets and LED counts for a specific device model.
///
/// These values are either extracted from the firmware schema or
/// fall back to hardcoded defaults (Scarlett 2i2 4th Gen).
#[derive(Debug, Clone)]
pub struct DeviceOffsets {
    /// Offset of `enableDirectLEDMode` (u8). 77 on the 2i2, 72 on the Solo.
    pub enable_direct_led: u32,
    /// Offset of `directLEDValues` (u32 array).
    pub direct_led_values: u32,
    /// Number of `directLEDValues` entries.
    pub direct_led_count: usize,
    /// DATA_NOTIFY event ID for `directLEDValues` changes.
    pub direct_led_notify: u32,
    /// Offset of `directLEDColour` (u32) — the single-LED write path.
    pub direct_led_colour: u32,
    /// Offset of `directLEDIndex` (u8) — the single-LED write path.
    pub direct_led_index: u32,
    /// DATA_NOTIFY event ID for `directLEDColour`/`directLEDIndex`.
    pub direct_led_colour_notify: u32,
    /// Offset of `selectedInput` (u8), or `None` when the model has no such
    /// control. `None` means number-LED restore cannot distinguish a selected
    /// input and must use the unselected colour for every number.
    pub selected_input: Option<u32>,
    /// Preamp gain control, or `None` when the model has none known. Muting
    /// then only lights the LEDs.
    pub preamp_gain: Option<PreampGain>,
}

/// Treat zero as absent, for both descriptor offsets and DATA_NOTIFY event
/// IDs. Extraction rejects a schema missing these members, so zero can only
/// reach here from a hand-edited or truncated cache file; writing to descriptor
/// offset 0 would corrupt unrelated state, and notifying event 0 is not a real
/// event on any known model.
fn nonzero_or(value: u32, fallback: u32, field: &str) -> u32 {
    if value == 0 {
        log::warn!("[offsets] schema gave 0 for {field}; using the built-in default");
        return fallback;
    }
    value
}

impl DeviceOffsets {
    /// Create offsets from firmware schema constants.
    ///
    /// Every field comes from the schema. The built-in 2i2 values are used only
    /// as a guard against a zero, which extraction rejects and so can reach here
    /// only from a truncated or hand-edited cache.
    pub fn from_schema(sc: &SchemaConstants) -> Self {
        Self {
            enable_direct_led: nonzero_or(
                sc.enable_direct_led_offset,
                protocol::OFF_ENABLE_DIRECT_LED,
                "enableDirectLEDMode",
            ),
            direct_led_values: sc.direct_led_offset,
            direct_led_count: sc.direct_led_count,
            direct_led_notify: nonzero_or(
                sc.direct_led_notify,
                protocol::NOTIFY_DIRECT_LED_VALUES,
                "directLEDValues notify-device",
            ),
            direct_led_colour: nonzero_or(
                sc.direct_led_colour_offset,
                protocol::OFF_DIRECT_LED_COLOUR,
                "directLEDColour",
            ),
            direct_led_index: nonzero_or(
                sc.direct_led_index_offset,
                protocol::OFF_DIRECT_LED_INDEX,
                "directLEDIndex",
            ),
            direct_led_colour_notify: nonzero_or(
                sc.direct_led_colour_notify,
                protocol::NOTIFY_DIRECT_LED_COLOUR,
                "directLEDColour notify-device",
            ),
            selected_input: sc.selected_input_offset,
            preamp_gain: sc.preamp_gain,
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
            direct_led_colour: protocol::OFF_DIRECT_LED_COLOUR,
            direct_led_index: protocol::OFF_DIRECT_LED_INDEX,
            direct_led_colour_notify: protocol::NOTIFY_DIRECT_LED_COLOUR,
            selected_input: Some(protocol::OFF_SELECTED_INPUT),
            // Never guessed: the 2.x parameter buffer offsets land inside
            // directLEDValues on 3.x firmware. Profiles and schemas set it.
            preamp_gain: None,
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
        assert_eq!(offsets.direct_led_colour, protocol::OFF_DIRECT_LED_COLOUR);
        assert_eq!(offsets.direct_led_index, protocol::OFF_DIRECT_LED_INDEX);
        assert_eq!(
            offsets.direct_led_colour_notify,
            protocol::NOTIFY_DIRECT_LED_COLOUR
        );
        assert_eq!(offsets.selected_input, Some(protocol::OFF_SELECTED_INPUT));
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
            enable_direct_led_offset: 77,
            direct_led_notify: 5,
            direct_led_colour_offset: 88,
            direct_led_index_offset: 92,
            direct_led_colour_notify: 8,
            selected_input_offset: Some(400),
            preamp_gain: None,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
            schema_format_version: 0,
        };
        let offsets = DeviceOffsets::from_schema(&sc);
        assert_eq!(offsets.direct_led_values, 100);
        assert_eq!(offsets.direct_led_count, 56);
        assert_eq!(offsets.direct_led_colour, 88);
        assert_eq!(offsets.direct_led_index, 92);
        assert_eq!(offsets.direct_led_colour_notify, 8);
        assert_eq!(offsets.selected_input, Some(400));
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
            // The 2i2's real APP_SPACE values, from docs/device_firmware_schema.json.
            enable_direct_led_offset: 77,
            direct_led_notify: 5,
            direct_led_colour_offset: 84,
            direct_led_index_offset: 88,
            direct_led_colour_notify: 8,
            selected_input_offset: Some(331),
            preamp_gain: None,
            metering_segments: 25,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: "2.0.2417.0".into(),
            schema_format_version: crate::schema::SCHEMA_FORMAT_VERSION,
        };
        let from_schema = DeviceOffsets::from_schema(&sc);
        let default = DeviceOffsets::default();
        assert_eq!(from_schema.enable_direct_led, default.enable_direct_led);
        assert_eq!(from_schema.direct_led_values, default.direct_led_values);
        assert_eq!(from_schema.direct_led_count, default.direct_led_count);
        assert_eq!(from_schema.direct_led_notify, default.direct_led_notify);
        assert_eq!(from_schema.direct_led_colour, default.direct_led_colour);
        assert_eq!(from_schema.direct_led_index, default.direct_led_index);
        assert_eq!(
            from_schema.direct_led_colour_notify,
            default.direct_led_colour_notify
        );
        assert_eq!(from_schema.selected_input, default.selected_input);
    }

    /// A model whose APP_SPACE is more compact than the 2i2's puts the
    /// single-LED fields four bytes earlier and has no `selectedInput` at all.
    /// Offsets observed on a Scarlett Solo 4th Gen (see docs/13, "Other models").
    #[test]
    fn from_schema_compact_layout_does_not_inherit_2i2_offsets() {
        let sc = SchemaConstants {
            product_name: "Scarlett Solo 4th Gen".into(),
            max_leds: 32,
            max_inputs: 2,
            direct_led_count: 32,
            direct_led_offset: 88,
            enable_direct_led_offset: 77,
            direct_led_notify: 5,
            direct_led_colour_offset: 80,
            direct_led_index_offset: 84,
            direct_led_colour_notify: 8,
            selected_input_offset: None,
            preamp_gain: None,
            ..SchemaConstants::default()
        };
        let offsets = DeviceOffsets::from_schema(&sc);
        let default = DeviceOffsets::default();

        assert_eq!(offsets.direct_led_colour, 80);
        assert_eq!(offsets.direct_led_index, 84);
        assert_ne!(offsets.direct_led_colour, default.direct_led_colour);
        assert_ne!(offsets.direct_led_index, default.direct_led_index);
        // The colour field must not land on the 2i2's index field.
        assert_ne!(offsets.direct_led_colour, default.direct_led_index);
        assert_eq!(offsets.selected_input, None);
    }

    /// Extraction rejects a schema missing these members, so a zero can only
    /// come from a truncated or hand-edited cache. Writing to descriptor
    /// offset 0 would corrupt unrelated state, so fall back instead.
    #[test]
    fn zero_offsets_fall_back_to_protocol_defaults() {
        let offsets = DeviceOffsets::from_schema(&SchemaConstants {
            direct_led_offset: 92,
            direct_led_count: 40,
            ..SchemaConstants::default()
        });
        assert_eq!(offsets.direct_led_colour, protocol::OFF_DIRECT_LED_COLOUR);
        assert_eq!(offsets.direct_led_index, protocol::OFF_DIRECT_LED_INDEX);
        assert_eq!(
            offsets.direct_led_colour_notify,
            protocol::NOTIFY_DIRECT_LED_COLOUR
        );
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
            enable_direct_led_offset: 77,
            direct_led_notify: 5,
            direct_led_colour_offset: 88,
            direct_led_index_offset: 92,
            direct_led_colour_notify: 8,
            selected_input_offset: Some(400),
            preamp_gain: None,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
            schema_format_version: 0,
        });
        let cloned = offsets.clone();
        assert_eq!(cloned.enable_direct_led, offsets.enable_direct_led);
        assert_eq!(cloned.direct_led_values, offsets.direct_led_values);
        assert_eq!(cloned.direct_led_count, offsets.direct_led_count);
        assert_eq!(cloned.direct_led_notify, offsets.direct_led_notify);
        assert_eq!(cloned.direct_led_colour, offsets.direct_led_colour);
        assert_eq!(cloned.direct_led_index, offsets.direct_led_index);
        assert_eq!(cloned.selected_input, offsets.selected_input);
    }

    #[test]
    fn debug_format_contains_field_names() {
        let offsets = DeviceOffsets::default();
        let debug = format!("{offsets:?}");
        assert!(debug.contains("enable_direct_led"));
        assert!(debug.contains("direct_led_count"));
    }
}
