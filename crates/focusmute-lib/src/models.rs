//! Model profiles — LED layouts for Scarlett 4th Gen devices.
//!
//! Each profile defines the input/output halo LED index ranges for a
//! specific model. Unknown models get `None` from `detect_model()`,
//! which callers should treat as "all halos" fallback.

use std::ops::Range;

use crate::offsets::DeviceOffsets;
use crate::protocol;

/// Where a profile's values came from, and therefore how far to trust them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    /// Every value was checked against the device on this project's bench.
    Verified,
    /// Reported by the named third party and not reproduced here. Callers warn
    /// and point at `map`, because nobody here has seen these LEDs light up.
    Reported(&'static str),
}

/// Firmware color for the currently-selected input's number LED.
///
/// The firmware drives number LEDs directly to hardware without updating
/// `directLEDValues`, so the actual color value is not readable from any
/// descriptor. The raw firmware value appears to be `0x40FF_0000` based on
/// visual observation, but that renders too light/washed-out when written
/// back via DATA_NOTIFY(8). `0x20FF_0000` was chosen to more closely match
/// the visual appearance of the firmware's native green.
///
/// Common across all Scarlett 4th Gen models. Used as fallback when
/// no `ModelProfile` is available (predicted layout path).
pub const DEFAULT_NUMBER_LED_SELECTED: u32 = 0x20FF_0000;

/// Firmware color for unselected input number LEDs.
///
/// Common across all Scarlett 4th Gen models. Used as fallback when
/// no `ModelProfile` is available (predicted layout path).
pub const DEFAULT_NUMBER_LED_UNSELECTED: u32 = 0x88FF_FF00;

/// LED index range for a single input or output halo.
#[derive(Debug)]
pub struct HaloRange {
    /// Index of the number indicator LED ("1", "2", etc.).
    pub number_led: usize,
    /// Index range of the halo ring segments.
    pub segments: Range<usize>,
}

/// LED layout profile for a specific Scarlett 4th Gen model.
#[derive(Debug)]
pub struct ModelProfile {
    pub name: &'static str,
    pub input_count: usize,
    pub led_count: usize,
    pub input_halos: &'static [HaloRange],
    pub output_halo_segments: Range<usize>,
    /// Button/indicator LED names (indices after output halo).
    /// Confirmed by hardware testing — cannot be derived from schema.
    pub button_labels: &'static [&'static str],
    /// Default colors for cache-dependent button LEDs.
    ///
    /// These LEDs read their color from `directLEDValues` in mode 0.
    /// After direct LED mode, stale data may remain in these positions.
    /// Writing these defaults + DATA_NOTIFY(5) restores them.
    ///
    /// Format: `(LED_index, default_color_0xRRGGBB00)`.
    /// Confirmed firmware values read from the device descriptor.
    pub cache_dependent_buttons: &'static [(usize, u32)],

    /// Firmware color for the currently-selected input's number LED.
    ///
    /// Firmware drives number LEDs based on `selectedInput` state — this is
    /// the color shown for the active input. The raw firmware value appears
    /// to be `0x40FF_0000` but that renders too light via DATA_NOTIFY(8);
    /// `0x20FF_0000` more closely matches the visual appearance.
    pub number_led_selected: u32,

    /// Firmware color for unselected input number LEDs.
    ///
    /// Visual approximation. Unselected inputs appear white on the 2i2.
    pub number_led_unselected: u32,

    /// Descriptor offsets for this model's LED write path.
    ///
    /// A profile short-circuits schema extraction, so it has to carry its own
    /// offsets. Inheriting a shared default would hand every profiled model the
    /// 2i2's numbers, which is exactly the bug the schema lookup exists to stop.
    pub offsets: &'static DeviceOffsets,

    /// Provenance of everything above.
    pub source: ProfileSource,
}

impl ModelProfile {
    /// A warning to surface to the user, or `None` for a verified profile.
    pub fn provenance_warning(&self) -> Option<String> {
        match self.source {
            ProfileSource::Verified => None,
            ProfileSource::Reported(who) => Some(format!(
                "{} LED layout is reported by {who} and unverified here; \
                 run `focusmute-cli map` to confirm the indices",
                self.name
            )),
        }
    }
}

// ── Scarlett 2i2 4th Gen ──

static SCARLETT_2I2_INPUT_HALOS: [HaloRange; 2] = [
    HaloRange {
        number_led: 0,
        segments: 1..8,
    }, // Input 1
    HaloRange {
        number_led: 8,
        segments: 9..16,
    }, // Input 2
];

/// Cache-dependent button defaults for Scarlett 2i2 4th Gen.
///
/// Self-coloring buttons (Inst=28, 48V=29, Air=30, Safe=32, Direct=33-34,36)
/// are driven by firmware directly and don't need defaults here.
static SCARLETT_2I2_CACHE_BUTTONS: [(usize, u32); 6] = [
    (27, 0x7080_8800), // Select 1 — white (firmware value)
    (31, 0x7080_8800), // Auto — white (firmware value)
    (35, 0x7080_8800), // Select 2 — white (firmware value)
    (37, 0x7080_8800), // Output 1 — white (firmware value)
    (38, 0x7080_8800), // Output 2 — white (firmware value)
    (39, 0x0038_0000), // USB — green (firmware value)
];

/// The 2i2's own APP_SPACE offsets. Asserted equal to `DeviceOffsets::default()`
/// and to what the 2i2 firmware schema yields, in the tests below.
static SCARLETT_2I2_OFFSETS: DeviceOffsets = DeviceOffsets {
    enable_direct_led: protocol::OFF_ENABLE_DIRECT_LED,
    direct_led_values: protocol::OFF_DIRECT_LED_VALUES,
    direct_led_count: protocol::DIRECT_LED_COUNT,
    direct_led_notify: protocol::NOTIFY_DIRECT_LED_VALUES,
    direct_led_colour: protocol::OFF_DIRECT_LED_COLOUR,
    direct_led_index: protocol::OFF_DIRECT_LED_INDEX,
    direct_led_colour_notify: protocol::NOTIFY_DIRECT_LED_COLOUR,
    selected_input: Some(protocol::OFF_SELECTED_INPUT),
};

static SCARLETT_2I2: ModelProfile = ModelProfile {
    name: "Scarlett 2i2 4th Gen",
    input_count: 2,
    led_count: 40,
    input_halos: &SCARLETT_2I2_INPUT_HALOS,
    output_halo_segments: 16..27,
    number_led_selected: 0x20FF_0000, // Green (firmware is 0x40FF, adjusted to match visually)
    number_led_unselected: 0xAAFF_DD00, // White (tuned to match firmware appearance)
    button_labels: &[
        "Select button LED 1",         // 27
        "Inst button",                 // 28
        "48V button",                  // 29
        "Air button",                  // 30
        "Auto button",                 // 31
        "Safe button",                 // 32
        "Direct button LED 1",         // 33
        "Direct button LED 2",         // 34
        "Select button LED 2",         // 35
        "Direct button crossed rings", // 36
        "Output indicator LED 1",      // 37
        "Output indicator LED 2",      // 38
        "USB symbol",                  // 39
    ],
    cache_dependent_buttons: &SCARLETT_2I2_CACHE_BUTTONS,
    offsets: &SCARLETT_2I2_OFFSETS,
    source: ProfileSource::Verified,
};

// ── Scarlett Solo 4th Gen ──
//
// Reported by SunsetSH/focusmute, an Apache-2.0 derivative of this project,
// from a panel sweep of one device (its docs/20-ledtest.md). Not reproduced
// here: there is no Solo on this bench. `ProfileSource::Reported` makes the
// app say so at runtime. Correcting an index means editing this profile by
// hand: `map` walks the panel and reports what each LED really is, but
// `--output-code` prints the schema *prediction*, not the corrected map.
//
// A second Solo, reported by HevarHal/Focusmute-Solo-Build, confirms numbers
// 4 and 12 (the sweep missed 12) and puts halo segments at 5-11 and 13-19.
// The sweep also missed 5 and 13, likely because the firmware repaints halos.

/// Offsets observed on a Solo running firmware 2.0.2417.0. Every one differs
/// from the 2i2's; see docs/13 § "Other 4th Gen Models".
static SCARLETT_SOLO_OFFSETS: DeviceOffsets = DeviceOffsets {
    enable_direct_led: 72,
    direct_led_values: 88,
    direct_led_count: 32,
    direct_led_notify: protocol::NOTIFY_DIRECT_LED_VALUES,
    direct_led_colour: 80,
    direct_led_index: 84,
    direct_led_colour_notify: protocol::NOTIFY_DIRECT_LED_COLOUR,
    // The Solo has no input-select control, so no number is ever "selected".
    selected_input: None,
};

static SCARLETT_SOLO_INPUT_HALOS: [HaloRange; 2] = [
    HaloRange {
        number_led: 4,
        segments: 5..12,
    }, // Input 1 (instrument)
    HaloRange {
        number_led: 12,
        segments: 13..20,
    }, // Input 2 (mic)
];

static SCARLETT_SOLO: ModelProfile = ModelProfile {
    name: "Scarlett Solo 4th Gen",
    input_count: 2,
    led_count: 32,
    input_halos: &SCARLETT_SOLO_INPUT_HALOS,
    // The Solo's Output indicator is a two-segment button (LEDs 24-25), not a
    // metering ring, so there is no output halo. The range is empty but anchored
    // past the input zone: `model_labels` derives the first button index from
    // `output_halo_segments.end`, and 0..0 would place buttons over the inputs.
    output_halo_segments: 20..20,
    // No selectedInput: `restore_number_leds` returns every number to the
    // unselected colour, and this value can never be chosen. It is set to the
    // same white so that a future caller reading it cannot paint an input green
    // on a device that has no notion of a selected input.
    number_led_selected: 0xAAFF_DD00,
    // Carried over from the 2i2, where it was tuned by eye against firmware
    // white. Not re-tuned against a Solo panel.
    number_led_unselected: 0xAAFF_DD00,
    // Air (0), Inst (2), 48V (3), Output (24-25), USB (26) and Direct (27, 31)
    // were all observed, but this struct places buttons contiguously after the
    // output halo and the Solo interleaves them with the input zone. Leaving
    // this empty keeps `map` from printing wrong labels; docs/13 carries the
    // real positions.
    button_labels: &[],
    // Restoring these needs DATA_NOTIFY(5), which blanks independent LEDs on a
    // Solo with no reliable way back. Deliberately empty.
    cache_dependent_buttons: &[],
    offsets: &SCARLETT_SOLO_OFFSETS,
    source: ProfileSource::Reported("SunsetSH/focusmute and HevarHal/Focusmute-Solo-Build"),
};

/// Detect the model profile from a model name.
///
/// Accepts the cleaned model name (e.g. "Scarlett 2i2 4th Gen") — callers
/// should use `DeviceInfo::model()` to strip the serial suffix.
/// Returns `None` for unknown models — callers should fall back to
/// the "all halos" gradient approach.
pub fn detect_model(model_name: &str) -> Option<&'static ModelProfile> {
    if model_name.eq_ignore_ascii_case("Scarlett 2i2 4th Gen") {
        return Some(&SCARLETT_2I2);
    }
    if model_name.eq_ignore_ascii_case("Scarlett Solo 4th Gen") {
        return Some(&SCARLETT_SOLO);
    }
    // Future: add 4i4, etc.
    None
}

/// Generate LED labels from a model profile and button names.
///
/// Derives input halo and output halo labels from the profile's
/// `input_halos` and `output_halo_segments`. Button names are placed
/// at indices starting after the output halo. Any remaining indices
/// get a generic "LED N" fallback.
pub fn model_labels(profile: &ModelProfile, button_names: &[&str]) -> Vec<String> {
    let mut labels = vec![String::new(); profile.led_count];

    // Input halos (number indicator + halo segments per input)
    for (input_idx, halo) in profile.input_halos.iter().enumerate() {
        let input_num = input_idx + 1;
        if halo.number_led < profile.led_count {
            labels[halo.number_led] = format!("Input {input_num} — \"{input_num}\" number");
        }
        for (seg_idx, led_idx) in halo.segments.clone().enumerate() {
            if led_idx < profile.led_count {
                labels[led_idx] = format!("Input {input_num} — Halo segment {}", seg_idx + 1);
            }
        }
    }

    // Output halo segments
    for (seg_idx, led_idx) in profile.output_halo_segments.clone().enumerate() {
        if led_idx < profile.led_count {
            labels[led_idx] = format!("Output — Halo segment {}", seg_idx + 1);
        }
    }

    // Buttons (placed after output halo)
    let first_button = profile.output_halo_segments.end;
    for (btn_idx, &name) in button_names.iter().enumerate() {
        let led_idx = first_button + btn_idx;
        if led_idx < profile.led_count {
            labels[led_idx] = name.to_string();
        }
    }

    // Fill remaining empty slots with generic fallback
    for (i, label) in labels.iter_mut().enumerate() {
        if label.is_empty() {
            *label = format!("LED {i}");
        }
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_model ──

    #[test]
    fn detect_2i2() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        assert_eq!(profile.name, "Scarlett 2i2 4th Gen");
        assert_eq!(profile.input_count, 2);
        assert_eq!(profile.led_count, 40);
    }

    #[test]
    fn detect_2i2_case_insensitive() {
        assert!(detect_model("scarlett 2i2 4th gen").is_some());
        assert!(detect_model("SCARLETT 2I2 4TH GEN").is_some());
    }

    // ── Scarlett Solo 4th Gen (reported profile) ──

    #[test]
    fn detect_solo_returns_the_reported_profile() {
        let profile = detect_model("Scarlett Solo 4th Gen").expect("Solo profile exists");
        assert_eq!(profile.name, "Scarlett Solo 4th Gen");
        assert_eq!(profile.input_count, 2);
        assert_eq!(profile.led_count, 32);
        assert_eq!(
            profile.source,
            ProfileSource::Reported("SunsetSH/focusmute and HevarHal/Focusmute-Solo-Build")
        );
    }

    /// The whole point of the profile: mute lands on the numbers the sweep saw,
    /// not the 0 and 8 a positional prediction produces.
    #[test]
    fn solo_profile_targets_the_reported_number_leds() {
        let profile = detect_model("Scarlett Solo 4th Gen").unwrap();
        let numbers: Vec<usize> = profile.input_halos.iter().map(|h| h.number_led).collect();
        assert_eq!(numbers, vec![4, 12]);
        let two_i_two = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let predicted: Vec<usize> = two_i_two.input_halos.iter().map(|h| h.number_led).collect();
        assert_ne!(numbers, predicted, "must not reuse the 2i2 positions");
    }

    /// Each halo is seven segments right after its number, as on the 2i2.
    #[test]
    fn solo_map_labels_seven_segment_halos() {
        let profile = detect_model("Scarlett Solo 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        assert_eq!(labels[5], "Input 1 — Halo segment 1");
        assert_eq!(labels[11], "Input 1 — Halo segment 7");
        assert_eq!(labels[13], "Input 2 — Halo segment 1");
        assert_eq!(labels[19], "Input 2 — Halo segment 7");
    }

    /// An unverified profile has to say so wherever it is used.
    #[test]
    fn reported_profile_warns_and_verified_one_does_not() {
        let solo = detect_model("Scarlett Solo 4th Gen").unwrap();
        let warning = solo
            .provenance_warning()
            .expect("reported profile must warn");
        assert!(warning.contains("SunsetSH/focusmute"), "warning: {warning}");
        assert!(
            warning.contains("map"),
            "warning should point at map: {warning}"
        );

        assert_eq!(
            detect_model("Scarlett 2i2 4th Gen")
                .unwrap()
                .provenance_warning(),
            None
        );
    }

    /// Restoring these needs DATA_NOTIFY(5), which blanks independent LEDs on a
    /// Solo with no reliable way back (docs/13). Both lists stay empty.
    #[test]
    fn solo_profile_declares_no_bulk_restore_state() {
        let profile = detect_model("Scarlett Solo 4th Gen").unwrap();
        assert!(profile.cache_dependent_buttons.is_empty());
        assert!(profile.button_labels.is_empty());
        assert!(profile.output_halo_segments.is_empty());
    }

    /// The Solo has no input-select control, so neither number colour may be
    /// the green the 2i2 uses for its selected input.
    #[test]
    fn solo_profile_never_paints_a_selected_input_green() {
        let profile = detect_model("Scarlett Solo 4th Gen").unwrap();
        assert_eq!(profile.number_led_selected, profile.number_led_unselected);
        assert_ne!(profile.number_led_selected, DEFAULT_NUMBER_LED_SELECTED);
        assert_eq!(profile.offsets.selected_input, None);
    }

    // ── Profile-owned offsets ──

    #[test]
    fn each_profile_carries_its_own_write_path() {
        let two_i_two = detect_model("Scarlett 2i2 4th Gen").unwrap().offsets;
        let solo = detect_model("Scarlett Solo 4th Gen").unwrap().offsets;
        for (a, b) in [
            (two_i_two.direct_led_colour, solo.direct_led_colour),
            (two_i_two.direct_led_index, solo.direct_led_index),
            (two_i_two.direct_led_values, solo.direct_led_values),
            (two_i_two.enable_direct_led, solo.enable_direct_led),
        ] {
            assert_ne!(a, b, "the two models must not share this offset");
        }
        assert_eq!(solo.direct_led_colour, 80);
        assert_eq!(solo.direct_led_index, 84);
        assert_eq!(solo.direct_led_values, 88);
        assert_eq!(solo.enable_direct_led, 72);
        assert_eq!(solo.direct_led_count, 32);
    }

    /// The profile's copy and the firmware schema are two statements of the
    /// same fact. If they ever disagree, one of them is wrong.
    #[test]
    fn the_2i2_profile_offsets_match_what_its_schema_yields() {
        let from_schema = DeviceOffsets::from_schema(&crate::schema::SchemaConstants {
            product_name: "Scarlett 2i2 4th Gen".into(),
            max_leds: 40,
            max_inputs: 2,
            max_outputs: 2,
            direct_led_count: 40,
            direct_led_offset: 92,
            direct_led_colour_offset: 84,
            direct_led_index_offset: 88,
            direct_led_colour_notify: 8,
            enable_direct_led_offset: 77,
            direct_led_notify: 5,
            selected_input_offset: Some(331),
            ..Default::default()
        });
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap().offsets;
        assert_eq!(profile.direct_led_colour, from_schema.direct_led_colour);
        assert_eq!(profile.direct_led_index, from_schema.direct_led_index);
        assert_eq!(profile.direct_led_values, from_schema.direct_led_values);
        assert_eq!(profile.direct_led_count, from_schema.direct_led_count);
        assert_eq!(profile.enable_direct_led, from_schema.enable_direct_led);
        assert_eq!(profile.direct_led_notify, from_schema.direct_led_notify);
        assert_eq!(profile.selected_input, from_schema.selected_input);
    }

    #[test]
    fn detect_unknown_model_returns_none() {
        assert!(detect_model("Scarlett 4i4 4th Gen").is_none());
        assert!(detect_model("Unknown Device").is_none());
        assert!(detect_model("").is_none());
    }

    // ── HaloRange bounds ──

    #[test]
    fn input1_halo_range() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let h = &profile.input_halos[0];
        assert_eq!(h.number_led, 0);
        assert_eq!(h.segments, 1..8);
        assert_eq!(h.segments.len(), 7);
    }

    #[test]
    fn input2_halo_range() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let h = &profile.input_halos[1];
        assert_eq!(h.number_led, 8);
        assert_eq!(h.segments, 9..16);
        assert_eq!(h.segments.len(), 7);
    }

    #[test]
    fn output_halo_range() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        assert_eq!(profile.output_halo_segments, 16..27);
        assert_eq!(profile.output_halo_segments.len(), 11);
    }

    #[test]
    fn input_count_matches_halos() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        assert_eq!(profile.input_count, profile.input_halos.len());
    }

    #[test]
    fn all_halo_indices_within_led_count() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        for halo in profile.input_halos {
            assert!(halo.number_led < profile.led_count);
            assert!(halo.segments.end <= profile.led_count);
        }
        assert!(profile.output_halo_segments.end <= profile.led_count);
    }

    #[test]
    fn halo_ranges_do_not_overlap() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        // Input 1 ends before Input 2 starts
        assert!(profile.input_halos[0].segments.end <= profile.input_halos[1].number_led);
        // Input 2 ends before Output starts
        assert!(profile.input_halos[1].segments.end <= profile.output_halo_segments.start);
    }

    /// The invariants above were written against the only profile that existed
    /// and never ran on any other. Every profile has to satisfy them, including
    /// the ordering one: `model_labels` computes the first button index from
    /// `output_halo_segments.end`, so a profile whose output range sits before
    /// its inputs would write button labels over the input zone.
    #[test]
    fn every_profile_satisfies_the_layout_invariants() {
        for name in ["Scarlett 2i2 4th Gen", "Scarlett Solo 4th Gen"] {
            let p = detect_model(name).unwrap_or_else(|| panic!("{name} profile missing"));
            assert_eq!(p.name, name, "profile name must match its lookup key");
            assert_eq!(
                p.input_halos.len(),
                p.input_count,
                "{name}: one halo range per input"
            );
            let mut cursor = 0usize;
            for (i, halo) in p.input_halos.iter().enumerate() {
                assert!(
                    halo.number_led < p.led_count,
                    "{name}: input {i} number LED"
                );
                assert!(
                    halo.segments.end <= p.led_count,
                    "{name}: input {i} halo end"
                );
                assert!(
                    halo.number_led >= cursor,
                    "{name}: input {i} number LED overlaps the previous input"
                );
                assert!(
                    halo.segments.start > halo.number_led,
                    "{name}: input {i} halo must follow its number LED"
                );
                cursor = halo.segments.end;
            }
            assert!(
                p.output_halo_segments.start >= cursor,
                "{name}: output halo must not precede the input zone"
            );
            assert!(
                p.output_halo_segments.end <= p.led_count,
                "{name}: output halo end"
            );
            assert!(
                p.output_halo_segments.end + p.button_labels.len() <= p.led_count,
                "{name}: button labels would run past the LED count"
            );
            for (idx, _) in p.cache_dependent_buttons {
                assert!(
                    *idx < p.led_count,
                    "{name}: cache button {idx} out of range"
                );
            }
        }
    }

    // ── model_labels ──

    #[test]
    fn model_labels_2i2_length() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        assert_eq!(labels.len(), 40);
    }

    #[test]
    fn model_labels_2i2_input_halos() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        // Input 1 number indicator
        assert_eq!(labels[0], "Input 1 — \"1\" number");
        // Input 1 halo segments
        assert_eq!(labels[1], "Input 1 — Halo segment 1");
        assert_eq!(labels[7], "Input 1 — Halo segment 7");
        // Input 2 number indicator
        assert_eq!(labels[8], "Input 2 — \"2\" number");
        // Input 2 halo segments
        assert_eq!(labels[9], "Input 2 — Halo segment 1");
        assert_eq!(labels[15], "Input 2 — Halo segment 7");
    }

    #[test]
    fn model_labels_2i2_output_halo() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        assert_eq!(labels[16], "Output — Halo segment 1");
        assert_eq!(labels[26], "Output — Halo segment 11");
    }

    #[test]
    fn model_labels_2i2_buttons() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        assert_eq!(labels[27], "Select button LED 1");
        assert_eq!(labels[28], "Inst button");
        assert_eq!(labels[39], "USB symbol");
    }

    #[test]
    fn model_labels_no_empty_entries() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let labels = model_labels(profile, profile.button_labels);
        for (i, label) in labels.iter().enumerate() {
            assert!(!label.is_empty(), "label at index {i} is empty");
        }
    }

    #[test]
    fn button_labels_count_matches_expected() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let expected_buttons = profile.led_count - profile.output_halo_segments.end;
        assert_eq!(profile.button_labels.len(), expected_buttons);
    }

    // ── cache_dependent_buttons ──

    #[test]
    fn cache_dependent_buttons_indices_within_range() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let first_button = profile.output_halo_segments.end;
        for &(idx, _color) in profile.cache_dependent_buttons {
            assert!(
                idx >= first_button && idx < profile.led_count,
                "cache-dep button index {idx} out of button range {first_button}..{}",
                profile.led_count
            );
        }
    }

    #[test]
    fn cache_dependent_buttons_have_nonzero_colors() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        for &(idx, color) in profile.cache_dependent_buttons {
            assert_ne!(color, 0, "cache-dep button at index {idx} has zero color");
        }
    }

    #[test]
    fn cache_dependent_buttons_no_duplicates() {
        let profile = detect_model("Scarlett 2i2 4th Gen").unwrap();
        let indices: Vec<usize> = profile
            .cache_dependent_buttons
            .iter()
            .map(|&(i, _)| i)
            .collect();
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                assert_ne!(
                    indices[i], indices[j],
                    "duplicate cache-dep button index {}",
                    indices[i]
                );
            }
        }
    }
}
