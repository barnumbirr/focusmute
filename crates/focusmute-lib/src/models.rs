//! Model profiles — LED layouts for Scarlett 4th Gen devices.
//!
//! Each profile defines the input/output halo LED index ranges for a
//! specific model. Unknown models get `None` from `detect_model()`,
//! which callers should treat as "all halos" fallback.

use std::ops::Range;

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
    // Future: add Solo, 4i4, etc.
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

    #[test]
    fn detect_unknown_model_returns_none() {
        assert!(detect_model("Scarlett Solo 4th Gen").is_none());
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
