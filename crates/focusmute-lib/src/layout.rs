//! LED layout prediction — infers LED positions from firmware schema constants.
//!
//! The firmware schema contains enough information to deterministically predict
//! the halo LED layout (inputs + output) and estimate button LED positions.
//! This eliminates the need for manual hardware testing for halo mapping and
//! provides useful predictions for button LEDs.
//!
//! ## Algorithm
//!
//! - Input halos: `max_inputs × 8 LEDs/input` (1 number indicator + 7 halo segments)
//! - Output halo: `metering_segments - (max_inputs × 7)` segments
//! - Buttons: remaining LEDs after all halos

use serde::{Deserialize, Serialize};

use crate::schema::SchemaConstants;

/// Halo ring segments per input — hardware constant across all Scarlett 4th Gen.
pub const HALO_SEGMENTS_PER_INPUT: usize = 7;

/// LEDs per input: 1 number indicator + 7 halo segments.
pub const LEDS_PER_INPUT: usize = 1 + HALO_SEGMENTS_PER_INPUT;

/// Confidence level for a predicted LED label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    /// Confirmed by hardcoded profile or known mapping.
    High,
    /// Inferred from schema with reasonable certainty (halos, known button patterns).
    Medium,
    /// Best guess — position known but label uncertain.
    Low,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::High => write!(f, "confirmed"),
            Confidence::Medium => write!(f, "predicted"),
            Confidence::Low => write!(f, "unknown"),
        }
    }
}

/// Which zone of the device an LED belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedZone {
    /// The number indicator LED for an input ("1", "2", etc.).
    InputNumber,
    /// A segment of an input's halo ring.
    InputHalo,
    /// A segment of the output halo ring.
    OutputHalo,
    /// A front-panel button or indicator LED.
    Button,
}

/// A single predicted LED with its label and confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedLed {
    pub index: usize,
    pub label: String,
    pub confidence: Confidence,
    pub zone: LedZone,
}

/// Complete predicted LED layout for a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedLayout {
    pub product_name: String,
    pub total_leds: usize,
    pub input_count: usize,
    pub output_halo_segments: usize,
    pub first_button_index: usize,
    pub button_count: usize,
    pub leds: Vec<PredictedLed>,
}

/// Known button labels derived from the Scarlett 2i2 4th Gen confirmed mapping.
///
/// The first N labels (matching the 2i2 button count) use the hardcoded profile
/// as ground truth. The confidence thresholds are: first 9 (Medium, schema-
/// verifiable controls), remaining 4 (Low, hardware-specific indicators).
const MEDIUM_CONFIDENCE_COUNT: usize = 9;

fn known_button_labels() -> Vec<(&'static str, Confidence)> {
    use crate::models;
    let profile = models::detect_model("Scarlett 2i2 4th Gen")
        .expect("2i2 profile must exist for known_button_labels");
    profile
        .button_labels
        .iter()
        .enumerate()
        .map(|(i, &label)| {
            let confidence = if i < MEDIUM_CONFIDENCE_COUNT {
                Confidence::Medium
            } else {
                Confidence::Low
            };
            (label, confidence)
        })
        .collect()
}

/// Predict the LED layout from schema constants.
///
/// Returns an error if the computed halo layout exceeds the total LED count.
pub fn predict_layout(schema: &SchemaConstants) -> crate::error::Result<PredictedLayout> {
    let total_leds = schema.max_leds;
    let input_count = schema.max_inputs;
    let total_input_leds = input_count * LEDS_PER_INPUT;

    // Output halo segments: prefer metering_segments arithmetic, fall back to gradient_count
    let output_halo_segments = if schema.metering_segments > 0 {
        let input_halo_total = input_count * HALO_SEGMENTS_PER_INPUT;
        if schema.metering_segments < input_halo_total {
            return Err(crate::FocusmuteError::Layout(format!(
                "metering_segments ({}) < input halo total ({}×{} = {})",
                schema.metering_segments, input_count, HALO_SEGMENTS_PER_INPUT, input_halo_total,
            )));
        }
        schema.metering_segments - input_halo_total
    } else {
        // Fallback: gradient_count is a reasonable proxy (equals 11 on 2i2)
        schema.gradient_count
    };

    let first_button_index = total_input_leds + output_halo_segments;
    if first_button_index > total_leds {
        return Err(crate::FocusmuteError::Layout(format!(
            "computed halo LEDs ({first_button_index}) exceed total LEDs ({total_leds}): \
             {input_count} inputs × {LEDS_PER_INPUT} + {output_halo_segments} output segments",
        )));
    }
    let button_count = total_leds - first_button_index;

    let mut leds = Vec::with_capacity(total_leds);

    // Input zone (HIGH confidence — layout is deterministic)
    for input_idx in 0..input_count {
        let base = input_idx * LEDS_PER_INPUT;
        // Number indicator
        leds.push(PredictedLed {
            index: base,
            label: format!("Input {} — \"{}\" number", input_idx + 1, input_idx + 1),
            confidence: Confidence::High,
            zone: LedZone::InputNumber,
        });
        // Halo segments
        for seg in 1..=HALO_SEGMENTS_PER_INPUT {
            leds.push(PredictedLed {
                index: base + seg,
                label: format!("Input {} — Halo segment {seg}", input_idx + 1),
                confidence: Confidence::High,
                zone: LedZone::InputHalo,
            });
        }
    }

    // Output halo zone (HIGH confidence)
    for seg in 1..=output_halo_segments {
        leds.push(PredictedLed {
            index: total_input_leds + seg - 1,
            label: format!("Output — Halo segment {seg}"),
            confidence: Confidence::High,
            zone: LedZone::OutputHalo,
        });
    }

    // Button zone — infer from known patterns
    let button_labels = infer_button_labels(
        button_count,
        &schema.input_controls,
        &schema.app_space_features,
    );
    for (i, (label, confidence)) in button_labels.into_iter().enumerate() {
        leds.push(PredictedLed {
            index: first_button_index + i,
            label,
            confidence,
            zone: LedZone::Button,
        });
    }

    Ok(PredictedLayout {
        product_name: schema.product_name.clone(),
        total_leds,
        input_count,
        output_halo_segments,
        first_button_index,
        button_count,
        leds,
    })
}

/// Infer button labels based on the button count and available schema controls.
fn infer_button_labels(
    button_count: usize,
    input_controls: &[String],
    app_space_features: &[String],
) -> Vec<(String, Confidence)> {
    // Build expected button list from schema hints
    let mut expected: Vec<(&str, Confidence)> = Vec::new();

    // Select button is present if selectedInput exists in APP_SPACE
    if app_space_features.iter().any(|f| f == "selectedInput") {
        expected.push(("Select button LED 1", Confidence::Medium));
    }

    // Input control buttons
    if input_controls.iter().any(|c| c == "instrument") {
        expected.push(("Inst button", Confidence::Medium));
    }
    if input_controls.iter().any(|c| c == "phantom-power") {
        expected.push(("48V button", Confidence::Medium));
    }
    if input_controls.iter().any(|c| c == "air") {
        expected.push(("Air button", Confidence::Medium));
    }
    if input_controls.iter().any(|c| c == "auto-gain") {
        expected.push(("Auto button", Confidence::Medium));
    }
    if input_controls.iter().any(|c| c == "clip-safe") {
        expected.push(("Safe button", Confidence::Medium));
    }

    // Direct monitoring button
    if app_space_features.iter().any(|f| f == "directMonitoring") {
        expected.push(("Direct button LED 1", Confidence::Medium));
        expected.push(("Direct button LED 2", Confidence::Medium));
    }

    // If selectedInput exists, second Select LED
    if app_space_features.iter().any(|f| f == "selectedInput") {
        expected.push(("Select button LED 2", Confidence::Medium));
    }

    // Direct crossed rings (if direct monitoring present)
    if app_space_features.iter().any(|f| f == "directMonitoring") {
        expected.push(("Direct button crossed rings", Confidence::Low));
    }

    // Output indicators and USB are common
    expected.push(("Output indicator LED 1", Confidence::Low));
    expected.push(("Output indicator LED 2", Confidence::Low));
    expected.push(("USB symbol", Confidence::Low));

    // If no controls info at all, fall back to the known pattern
    if input_controls.is_empty() && app_space_features.is_empty() {
        let known = known_button_labels();
        return if button_count <= known.len() {
            known[..button_count]
                .iter()
                .map(|(l, c)| (l.to_string(), *c))
                .collect()
        } else {
            let mut result: Vec<(String, Confidence)> =
                known.iter().map(|(l, c)| (l.to_string(), *c)).collect();
            for i in result.len()..button_count {
                result.push((format!("Button/indicator LED {}", i + 1), Confidence::Low));
            }
            result
        };
    }

    // Match expected to available button slots
    let mut result = Vec::with_capacity(button_count);
    for i in 0..button_count {
        if i < expected.len() {
            result.push((expected[i].0.to_string(), expected[i].1));
        } else {
            result.push((format!("Button/indicator LED {}", i + 1), Confidence::Low));
        }
    }
    result
}

/// Generate a pasteable Rust `ModelProfile` code snippet from a predicted layout.
pub fn generate_model_profile_code(layout: &PredictedLayout) -> String {
    let ident = layout.product_name.to_uppercase().replace([' ', '-'], "_");

    let mut code = String::new();

    // Input halos array
    code.push_str(&format!(
        "static {ident}_INPUT_HALOS: [HaloRange; {}] = [\n",
        layout.input_count
    ));
    for i in 0..layout.input_count {
        let base = i * LEDS_PER_INPUT;
        code.push_str(&format!(
            "    HaloRange {{ number_led: {}, segments: {}..{} }},  // Input {}\n",
            base,
            base + 1,
            base + 1 + HALO_SEGMENTS_PER_INPUT,
            i + 1,
        ));
    }
    code.push_str("];\n\n");

    // ModelProfile
    let output_start = layout.input_count * LEDS_PER_INPUT;
    let output_end = output_start + layout.output_halo_segments;
    code.push_str(&format!("static {ident}: ModelProfile = ModelProfile {{\n"));
    code.push_str(&format!("    name: \"{}\",\n", layout.product_name));
    code.push_str(&format!("    input_count: {},\n", layout.input_count));
    code.push_str(&format!("    led_count: {},\n", layout.total_leds));
    code.push_str(&format!("    input_halos: &{ident}_INPUT_HALOS,\n"));
    code.push_str(&format!(
        "    output_halo_segments: {output_start}..{output_end},\n"
    ));
    code.push_str("    button_labels: &[], // TODO: fill in button labels\n");
    code.push_str("};\n");

    code
}

/// Combine hardcoded labels, predicted labels, and generic fallbacks.
///
/// Returns a vec of `(label, confidence)` tuples for every LED index 0..total_leds.
/// Priority: hardcoded (High) > predicted > generic "LED N" (None).
pub fn resolve_labels<S: AsRef<str>>(
    hardcoded: Option<&[S]>,
    predicted: Option<&PredictedLayout>,
    total_leds: usize,
) -> Vec<(String, Option<Confidence>)> {
    // Build index-based lookup from predicted layout (O(n) instead of O(n²)).
    let predicted_by_index: Vec<Option<&PredictedLed>> = if let Some(layout) = predicted {
        let mut lookup = vec![None; total_leds];
        for led in &layout.leds {
            if led.index < total_leds {
                lookup[led.index] = Some(led);
            }
        }
        lookup
    } else {
        vec![None; total_leds]
    };

    (0..total_leds)
        .map(|i| {
            // Hardcoded takes priority
            if let Some(labels) = hardcoded
                && let Some(label) = labels.get(i)
            {
                return (label.as_ref().to_string(), Some(Confidence::High));
            }
            // Predicted next (O(1) lookup)
            if let Some(led) = predicted_by_index[i] {
                return (led.label.clone(), Some(led.confidence));
            }
            // Generic fallback
            (format!("LED {i}"), None)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build SchemaConstants matching Scarlett 2i2 4th Gen.
    fn schema_2i2() -> SchemaConstants {
        SchemaConstants {
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
            input_controls: vec![
                "air".into(),
                "instrument".into(),
                "phantom-power".into(),
                "clip-safe".into(),
                "auto-gain".into(),
            ],
            app_space_features: vec!["directMonitoring".into(), "selectedInput".into()],
            firmware_version: "2.0.2417.0".into(),
        }
    }

    #[test]
    fn predict_2i2_layout() {
        let layout = predict_layout(&schema_2i2()).unwrap();
        assert_eq!(layout.total_leds, 40);
        assert_eq!(layout.input_count, 2);
        assert_eq!(layout.output_halo_segments, 11); // 25 - 2*7
        assert_eq!(layout.first_button_index, 27); // 2*8 + 11
        assert_eq!(layout.button_count, 13); // 40 - 27
        assert_eq!(layout.leds.len(), 40);

        // Verify Input 1 number indicator
        assert_eq!(layout.leds[0].index, 0);
        assert_eq!(layout.leds[0].label, "Input 1 — \"1\" number");
        assert_eq!(layout.leds[0].confidence, Confidence::High);
        assert_eq!(layout.leds[0].zone, LedZone::InputNumber);

        // Verify Input 1 halo segments
        for seg in 1..=7 {
            assert_eq!(layout.leds[seg].index, seg);
            assert_eq!(
                layout.leds[seg].label,
                format!("Input 1 — Halo segment {seg}")
            );
            assert_eq!(layout.leds[seg].confidence, Confidence::High);
            assert_eq!(layout.leds[seg].zone, LedZone::InputHalo);
        }

        // Verify Input 2 number indicator at index 8
        assert_eq!(layout.leds[8].index, 8);
        assert_eq!(layout.leds[8].label, "Input 2 — \"2\" number");

        // Verify output halo starts at index 16
        assert_eq!(layout.leds[16].index, 16);
        assert_eq!(layout.leds[16].label, "Output — Halo segment 1");
        assert_eq!(layout.leds[16].confidence, Confidence::High);
        assert_eq!(layout.leds[16].zone, LedZone::OutputHalo);

        // Verify last output halo
        assert_eq!(layout.leds[26].index, 26);
        assert_eq!(layout.leds[26].label, "Output — Halo segment 11");

        // Verify first button at index 27
        assert_eq!(layout.leds[27].index, 27);
        assert_eq!(layout.leds[27].zone, LedZone::Button);
    }

    #[test]
    fn predict_hypothetical_4i4() {
        let schema = SchemaConstants {
            product_name: "Scarlett 4i4 4th Gen".into(),
            max_leds: 56,
            max_inputs: 4,
            max_outputs: 4,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 56,
            direct_led_offset: 92,
            metering_segments: 39, // 4*7 + 11
            input_controls: vec!["air".into(), "instrument".into()],
            app_space_features: vec!["directMonitoring".into()],
            firmware_version: String::new(),
        };
        let layout = predict_layout(&schema).unwrap();
        assert_eq!(layout.input_count, 4);
        assert_eq!(layout.output_halo_segments, 11); // 39 - 28
        assert_eq!(layout.first_button_index, 43); // 4*8 + 11
        assert_eq!(layout.button_count, 13); // 56 - 43
        assert_eq!(layout.leds.len(), 56);

        // Input 3 number indicator at index 16
        assert_eq!(layout.leds[16].index, 16);
        assert_eq!(layout.leds[16].label, "Input 3 — \"3\" number");

        // Input 4 number indicator at index 24
        assert_eq!(layout.leds[24].index, 24);
        assert_eq!(layout.leds[24].label, "Input 4 — \"4\" number");
    }

    #[test]
    fn predict_solo_1_input() {
        let schema = SchemaConstants {
            product_name: "Scarlett Solo 4th Gen".into(),
            max_leds: 22,
            max_inputs: 1,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 22,
            direct_led_offset: 92,
            metering_segments: 18, // 1*7 + 11
            input_controls: vec!["air".into(), "instrument".into()],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let layout = predict_layout(&schema).unwrap();
        assert_eq!(layout.input_count, 1);
        assert_eq!(layout.output_halo_segments, 11);
        assert_eq!(layout.first_button_index, 19); // 1*8 + 11
        assert_eq!(layout.button_count, 3); // 22 - 19
        assert_eq!(layout.leds.len(), 22);

        // Only 1 input number indicator
        assert_eq!(layout.leds[0].label, "Input 1 — \"1\" number");
    }

    #[test]
    fn predict_metering_segments_fallback() {
        // metering_segments=0 → uses gradient_count as fallback
        let schema = SchemaConstants {
            product_name: "Unknown Device".into(),
            max_leds: 40,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 40,
            direct_led_offset: 92,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let layout = predict_layout(&schema).unwrap();
        assert_eq!(layout.output_halo_segments, 11); // gradient_count fallback
        assert_eq!(layout.first_button_index, 27); // 2*8 + 11
    }

    #[test]
    fn predict_overflow_returns_error() {
        // More halo LEDs than total → should error
        let schema = SchemaConstants {
            product_name: "Bad Device".into(),
            max_leds: 10,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 10,
            direct_led_offset: 92,
            metering_segments: 25,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let result = predict_layout(&schema);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceed total LEDs")
        );
    }

    #[test]
    fn predict_metering_segments_less_than_input_halos_returns_error() {
        let schema = SchemaConstants {
            product_name: "Bad Device".into(),
            max_leds: 40,
            max_inputs: 4,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 40,
            direct_led_offset: 92,
            metering_segments: 10, // less than 4*7=28
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let result = predict_layout(&schema);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("metering_segments")
        );
    }

    #[test]
    fn predict_no_controls_low_confidence() {
        let schema = SchemaConstants {
            product_name: "Unknown Device".into(),
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
            firmware_version: String::new(),
        };
        let layout = predict_layout(&schema).unwrap();
        // With no control info, button labels fall back to known_button_labels()
        // Last few should be Low confidence
        let last_button = &layout.leds[layout.leds.len() - 1];
        assert_eq!(last_button.confidence, Confidence::Low);
    }

    #[test]
    fn generate_code_matches_2i2_shape() {
        let layout = predict_layout(&schema_2i2()).unwrap();
        let code = generate_model_profile_code(&layout);

        // Should contain the input halos array
        assert!(code.contains("SCARLETT_2I2_4TH_GEN_INPUT_HALOS"));
        assert!(code.contains("[HaloRange; 2]"));
        assert!(code.contains("number_led: 0"));
        assert!(code.contains("segments: 1..8"));
        assert!(code.contains("number_led: 8"));
        assert!(code.contains("segments: 9..16"));

        // Should contain the ModelProfile
        assert!(code.contains("static SCARLETT_2I2_4TH_GEN: ModelProfile"));
        assert!(code.contains("input_count: 2"));
        assert!(code.contains("led_count: 40"));
        assert!(!code.contains("gradient_count"));
        assert!(code.contains("output_halo_segments: 16..27"));
    }

    #[test]
    fn resolve_labels_hardcoded_takes_priority() {
        let hardcoded = &["Custom Label 0", "Custom Label 1"][..];
        let layout = predict_layout(&schema_2i2()).unwrap();
        let labels = resolve_labels(Some(hardcoded), Some(&layout), 3);

        // First two come from hardcoded
        assert_eq!(labels[0].0, "Custom Label 0");
        assert_eq!(labels[0].1, Some(Confidence::High));
        assert_eq!(labels[1].0, "Custom Label 1");
        assert_eq!(labels[1].1, Some(Confidence::High));
        // Third falls through to predicted
        assert_eq!(labels[2].0, "Input 1 — Halo segment 2");
        assert_eq!(labels[2].1, Some(Confidence::High));
    }

    #[test]
    fn resolve_labels_predicted_over_generic() {
        let layout = predict_layout(&schema_2i2()).unwrap();
        let labels = resolve_labels(None::<&[&str]>, Some(&layout), 2);

        assert_eq!(labels[0].0, "Input 1 — \"1\" number");
        assert_eq!(labels[0].1, Some(Confidence::High));
    }

    #[test]
    fn resolve_labels_generic_fallback() {
        let labels = resolve_labels(None::<&[&str]>, None, 3);
        assert_eq!(labels[0].0, "LED 0");
        assert_eq!(labels[0].1, None);
        assert_eq!(labels[2].0, "LED 2");
    }

    #[test]
    fn predict_zero_buttons() {
        // Exactly as many LEDs as halos — no buttons
        let schema = SchemaConstants {
            product_name: "Halo Only".into(),
            max_leds: 27,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 27,
            direct_led_offset: 92,
            metering_segments: 25,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: String::new(),
        };
        let layout = predict_layout(&schema).unwrap();
        assert_eq!(layout.button_count, 0);
        assert_eq!(layout.first_button_index, 27);
        assert_eq!(layout.leds.len(), 27);
    }
}
