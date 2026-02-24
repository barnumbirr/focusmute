//! Mute strategy resolution — determines how mute indication is visualized.

use std::collections::HashMap;

use crate::config::{Config, MuteInputs};
use crate::layout::{LedZone, PredictedLayout};
use crate::models::{self, ModelProfile};

use super::color::parse_color;

/// Resolved mute visualization strategy.
///
/// Targets specific input number LEDs via single-LED update (DATA_NOTIFY(8)).
/// Only the number indicator LEDs ("1", "2") change color — the metering
/// halo rings and all other LEDs are completely untouched.
#[derive(Debug, Clone)]
pub struct MuteStrategy {
    /// 0-indexed input indices to indicate as muted.
    pub input_indices: Vec<usize>,
    /// LED index of each muted input's number LED (e.g. 0 for "1", 8 for "2").
    pub number_leds: Vec<u8>,
    /// Per-input mute colors. Same length as `number_leds`. Falls back to
    /// the global mute color if empty. Allows different colors per input
    /// via `input_colors` config.
    pub mute_colors: Vec<u32>,
    /// Firmware color for the selected input's number LED (for restore).
    pub selected_color: u32,
    /// Firmware color for unselected input number LEDs (for restore).
    pub unselected_color: u32,
}

/// Extract number LED indices from a predicted layout.
///
/// Returns `(input_indices, number_leds)` for all LEDs with `zone == LedZone::InputNumber`.
fn number_leds_from_predicted(
    predicted: &PredictedLayout,
) -> Result<(Vec<usize>, Vec<u8>), String> {
    let mut input_indices = Vec::new();
    let mut number_leds = Vec::new();
    let mut input_idx = 0usize;
    for led in &predicted.leds {
        if led.zone == LedZone::InputNumber {
            input_indices.push(input_idx);
            let led_u8 = u8::try_from(led.index).map_err(|_| {
                format!(
                    "LED index {} exceeds u8 range for input {}",
                    led.index,
                    input_idx + 1
                )
            })?;
            number_leds.push(led_u8);
            input_idx += 1;
        }
    }
    Ok((input_indices, number_leds))
}

/// Resolve a [`MuteStrategy`] from config, model profile, and predicted layout.
///
/// Returns `Ok((strategy, optional_warning))` or `Err` if the device is unsupported.
///
/// Fallback chain: profile → predicted layout → error.
pub fn resolve_mute_strategy(
    mute_inputs: &MuteInputs,
    profile: Option<&ModelProfile>,
    predicted: Option<&PredictedLayout>,
    mute_color: u32,
    input_colors: &HashMap<String, String>,
) -> Result<(MuteStrategy, Option<String>), String> {
    match mute_inputs {
        MuteInputs::All => {
            if let Some(profile) = profile {
                // Known device: target all input number LEDs via DATA_NOTIFY(8).
                let input_indices: Vec<usize> = (0..profile.input_halos.len()).collect();
                let number_leds: Vec<u8> = profile
                    .input_halos
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        u8::try_from(h.number_led).map_err(|_| {
                            format!(
                                "number_led {} exceeds u8 range for input {}",
                                h.number_led,
                                i + 1
                            )
                        })
                    })
                    .collect::<Result<Vec<u8>, String>>()?;
                let mute_colors = build_mute_colors(&input_indices, mute_color, input_colors);
                Ok((
                    MuteStrategy {
                        input_indices,
                        number_leds,
                        mute_colors,
                        selected_color: profile.number_led_selected,
                        unselected_color: profile.number_led_unselected,
                    },
                    None,
                ))
            } else if let Some(predicted) = predicted {
                // Unknown device with schema: use predicted number LED indices.
                let (input_indices, number_leds) = number_leds_from_predicted(predicted)?;
                if input_indices.is_empty() {
                    return Err(
                        "predicted layout has no input number LEDs; device not supported".into(),
                    );
                }
                let mute_colors = build_mute_colors(&input_indices, mute_color, input_colors);
                Ok((
                    MuteStrategy {
                        input_indices,
                        number_leds,
                        mute_colors,
                        selected_color: models::DEFAULT_NUMBER_LED_SELECTED,
                        unselected_color: models::DEFAULT_NUMBER_LED_UNSELECTED,
                    },
                    Some("using predicted LED layout (no hardcoded profile)".into()),
                ))
            } else {
                Err("unknown device with no schema; cannot determine number LED indices".into())
            }
        }
        MuteInputs::Specific(indices) => {
            if let Some(profile) = profile {
                let mut valid_indices = Vec::new();
                let mut number_leds = Vec::new();
                for &idx in indices {
                    if idx < profile.input_halos.len() {
                        valid_indices.push(idx);
                        let led =
                            u8::try_from(profile.input_halos[idx].number_led).map_err(|_| {
                                format!(
                                    "number_led {} exceeds u8 range for input {}",
                                    profile.input_halos[idx].number_led,
                                    idx + 1
                                )
                            })?;
                        number_leds.push(led);
                    }
                }

                if valid_indices.is_empty() {
                    return Err(
                        "all specified input indices are out of range for this device".into(),
                    );
                }

                let mute_colors = build_mute_colors(&valid_indices, mute_color, input_colors);
                Ok((
                    MuteStrategy {
                        input_indices: valid_indices,
                        number_leds,
                        mute_colors,
                        selected_color: profile.number_led_selected,
                        unselected_color: profile.number_led_unselected,
                    },
                    None,
                ))
            } else if let Some(predicted) = predicted {
                let (all_indices, all_leds) = number_leds_from_predicted(predicted)?;
                let mut valid_indices = Vec::new();
                let mut number_leds = Vec::new();
                for &idx in indices {
                    if let Some(pos) = all_indices.iter().position(|&i| i == idx) {
                        valid_indices.push(idx);
                        number_leds.push(all_leds[pos]);
                    }
                }

                if valid_indices.is_empty() {
                    return Err(
                        "all specified input indices are out of range for predicted layout".into(),
                    );
                }

                let mute_colors = build_mute_colors(&valid_indices, mute_color, input_colors);
                Ok((
                    MuteStrategy {
                        input_indices: valid_indices,
                        number_leds,
                        mute_colors,
                        selected_color: models::DEFAULT_NUMBER_LED_SELECTED,
                        unselected_color: models::DEFAULT_NUMBER_LED_UNSELECTED,
                    },
                    Some("using predicted LED layout (no hardcoded profile)".into()),
                ))
            } else {
                Err("per-input mute requires a known model profile or schema; \
                     device not supported"
                    .into())
            }
        }
    }
}

/// Build per-input mute colors from config `input_colors` map, falling back to global color.
///
/// `input_indices` are 0-indexed; `input_colors` keys are 1-based strings (e.g. "1", "2").
fn build_mute_colors(
    input_indices: &[usize],
    global_mute_color: u32,
    input_colors: &HashMap<String, String>,
) -> Vec<u32> {
    input_indices
        .iter()
        .map(|&idx| {
            let key = (idx + 1).to_string();
            input_colors
                .get(&key)
                .and_then(|c| parse_color(c).ok())
                .unwrap_or(global_mute_color)
        })
        .collect()
}

// ── Shared helpers (DRY across cli.rs / tray.rs) ──

/// Parse the mute color from config, falling back to red on invalid input.
pub fn mute_color_or_default(config: &Config) -> u32 {
    parse_color(&config.mute_color).unwrap_or(0xFF00_0000)
}

/// Validate mute-inputs config, parse it, and resolve the mute strategy.
///
/// Returns `Ok((mute_mode, strategy, warnings))` or `Err` if the device is unsupported.
pub fn resolve_strategy_from_config(
    config: &mut Config,
    input_count: Option<usize>,
    profile: Option<&ModelProfile>,
    predicted: Option<&PredictedLayout>,
) -> Result<(MuteInputs, MuteStrategy, Vec<String>), String> {
    let mut warnings = Vec::new();
    if let Some(ic) = input_count
        && let Err(e) = config.validate_mute_inputs(ic)
    {
        warnings.push(format!("{e} — falling back to all inputs"));
        config.mute_inputs = "all".into();
    }
    let mute_mode = config.parse_mute_inputs();
    let mute_color = mute_color_or_default(config);
    let (strategy, strategy_warning) = resolve_mute_strategy(
        &mute_mode,
        profile,
        predicted,
        mute_color,
        &config.input_colors,
    )?;
    if let Some(w) = strategy_warning {
        warnings.push(w);
    }
    Ok((mute_mode, strategy, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;

    const RED: u32 = 0xFF00_0000;

    fn no_input_colors() -> HashMap<String, String> {
        HashMap::new()
    }

    // ── resolve_mute_strategy ──

    #[test]
    fn resolve_all_with_profile_returns_per_input_all() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let (strategy, warning) = resolve_mute_strategy(
            &MuteInputs::All,
            Some(profile),
            None,
            RED,
            &no_input_colors(),
        )
        .unwrap();
        assert!(warning.is_none());
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert_eq!(strategy.number_leds, &[0, 8]);
        assert_eq!(strategy.mute_colors, &[RED, RED]);
        assert_eq!(strategy.selected_color, 0x20FF_0000);
        assert_eq!(strategy.unselected_color, 0xFFFF_FF00);
    }

    #[test]
    fn resolve_all_no_profile_no_predicted_returns_error() {
        let result = resolve_mute_strategy(&MuteInputs::All, None, None, RED, &no_input_colors());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown device"));
    }

    #[test]
    fn resolve_all_with_predicted_layout() {
        let predicted = make_predicted_layout(2);
        let (strategy, warning) = resolve_mute_strategy(
            &MuteInputs::All,
            None,
            Some(&predicted),
            RED,
            &no_input_colors(),
        )
        .unwrap();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("predicted"));
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert_eq!(strategy.number_leds, &[0, 8]);
        assert_eq!(strategy.selected_color, models::DEFAULT_NUMBER_LED_SELECTED);
        assert_eq!(
            strategy.unselected_color,
            models::DEFAULT_NUMBER_LED_UNSELECTED
        );
    }

    #[test]
    fn resolve_specific_with_profile_returns_per_input() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let (strategy, warning) = resolve_mute_strategy(
            &MuteInputs::Specific(vec![0]),
            Some(profile),
            None,
            RED,
            &no_input_colors(),
        )
        .unwrap();
        assert!(warning.is_none());
        assert_eq!(strategy.input_indices, &[0]);
        assert_eq!(strategy.number_leds, &[0]);
        assert_eq!(strategy.mute_colors, &[RED]);
    }

    #[test]
    fn resolve_specific_both_inputs() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let (strategy, warning) = resolve_mute_strategy(
            &MuteInputs::Specific(vec![0, 1]),
            Some(profile),
            None,
            RED,
            &no_input_colors(),
        )
        .unwrap();
        assert!(warning.is_none());
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert_eq!(strategy.number_leds, &[0, 8]);
    }

    #[test]
    fn resolve_specific_invalid_indices_returns_error() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let result = resolve_mute_strategy(
            &MuteInputs::Specific(vec![5]),
            Some(profile),
            None,
            RED,
            &no_input_colors(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
    }

    #[test]
    fn resolve_specific_no_profile_no_predicted_returns_error() {
        let result = resolve_mute_strategy(
            &MuteInputs::Specific(vec![0]),
            None,
            None,
            RED,
            &no_input_colors(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolve_specific_with_predicted_layout() {
        let predicted = make_predicted_layout(2);
        let (strategy, warning) = resolve_mute_strategy(
            &MuteInputs::Specific(vec![0]),
            None,
            Some(&predicted),
            RED,
            &no_input_colors(),
        )
        .unwrap();
        assert!(warning.is_some());
        assert_eq!(strategy.input_indices, &[0]);
        assert_eq!(strategy.number_leds, &[0]);
    }

    #[test]
    fn resolve_per_input_custom_colors() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let input_colors = HashMap::from([
            ("1".into(), "#00FF00".into()), // green for input 1
            ("2".into(), "#0000FF".into()), // blue for input 2
        ]);
        let (strategy, warning) =
            resolve_mute_strategy(&MuteInputs::All, Some(profile), None, RED, &input_colors)
                .unwrap();
        assert!(warning.is_none());
        // Input 1 → green (0x00FF0000 in RGBW), Input 2 → blue (0x0000FF00 in RGBW)
        assert_eq!(strategy.mute_colors[0], parse_color("#00FF00").unwrap());
        assert_eq!(strategy.mute_colors[1], parse_color("#0000FF").unwrap());
    }

    #[test]
    fn resolve_per_input_partial_custom_colors_falls_back_to_global() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        // Only input 2 has a custom color
        let input_colors = HashMap::from([("2".into(), "#00FF00".into())]);
        let (strategy, _) =
            resolve_mute_strategy(&MuteInputs::All, Some(profile), None, RED, &input_colors)
                .unwrap();
        assert_eq!(strategy.mute_colors[0], RED); // falls back to global
        assert_eq!(strategy.mute_colors[1], parse_color("#00FF00").unwrap());
    }

    #[test]
    fn resolve_per_input_invalid_custom_color_falls_back_to_global() {
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let input_colors = HashMap::from([("1".into(), "not-a-color".into())]);
        let (strategy, _) =
            resolve_mute_strategy(&MuteInputs::All, Some(profile), None, RED, &input_colors)
                .unwrap();
        assert_eq!(strategy.mute_colors[0], RED); // invalid → falls back to global
        assert_eq!(strategy.mute_colors[1], RED);
    }

    // ── mute_color_or_default ──

    #[test]
    fn mute_color_or_default_valid_hex() {
        let mut config = Config::load();
        config.mute_color = "#00FF00".into();
        assert_eq!(mute_color_or_default(&config), 0x00FF_0000);
    }

    #[test]
    fn mute_color_or_default_invalid_returns_red() {
        let mut config = Config::load();
        config.mute_color = "garbage".into();
        assert_eq!(mute_color_or_default(&config), 0xFF00_0000);
    }

    // ── resolve_strategy_from_config ──

    #[test]
    fn resolve_strategy_all_inputs_with_profile() {
        let mut config = Config::load();
        config.mute_inputs = "all".into();
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let (mode, strategy, warnings) =
            resolve_strategy_from_config(&mut config, Some(2), Some(profile), None).unwrap();
        assert!(matches!(mode, MuteInputs::All));
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_strategy_all_inputs_no_profile_no_predicted_returns_error() {
        let mut config = Config::load();
        config.mute_inputs = "all".into();
        let result = resolve_strategy_from_config(&mut config, Some(2), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_strategy_validation_failure_falls_back() {
        let mut config = Config::load();
        config.mute_inputs = "5".into(); // out of range for 2 inputs
        let profile = models::detect_model("Scarlett 2i2 4th Gen").unwrap();
        let (mode, strategy, warnings) =
            resolve_strategy_from_config(&mut config, Some(2), Some(profile), None).unwrap();
        assert!(matches!(mode, MuteInputs::All));
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("falling back"));
        assert_eq!(config.mute_inputs, "all");
    }

    #[test]
    fn resolve_strategy_with_predicted_layout() {
        let mut config = Config::load();
        config.mute_inputs = "all".into();
        let predicted = make_predicted_layout(2);
        let (mode, strategy, warnings) =
            resolve_strategy_from_config(&mut config, Some(2), None, Some(&predicted)).unwrap();
        assert!(matches!(mode, MuteInputs::All));
        assert_eq!(strategy.input_indices, &[0, 1]);
        assert!(!warnings.is_empty()); // "using predicted" warning
    }

    // ── Test helpers ──

    /// Create a minimal predicted layout with the given input count.
    fn make_predicted_layout(input_count: usize) -> PredictedLayout {
        use crate::layout::{Confidence, LEDS_PER_INPUT, LedZone, PredictedLed};

        let total_leds = input_count * LEDS_PER_INPUT + 11 + 13; // halos + output + buttons
        let mut leds = Vec::new();

        for i in 0..input_count {
            let base = i * LEDS_PER_INPUT;
            leds.push(PredictedLed {
                index: base,
                label: format!("Input {} — \"{}\" number", i + 1, i + 1),
                confidence: Confidence::High,
                zone: LedZone::InputNumber,
            });
            for seg in 1..=7 {
                leds.push(PredictedLed {
                    index: base + seg,
                    label: format!("Input {} — Halo segment {seg}", i + 1),
                    confidence: Confidence::High,
                    zone: LedZone::InputHalo,
                });
            }
        }

        let output_start = input_count * LEDS_PER_INPUT;
        for seg in 0..11 {
            leds.push(PredictedLed {
                index: output_start + seg,
                label: format!("Output — Halo segment {}", seg + 1),
                confidence: Confidence::High,
                zone: LedZone::OutputHalo,
            });
        }

        let button_start = output_start + 11;
        for i in 0..13 {
            leds.push(PredictedLed {
                index: button_start + i,
                label: format!("Button {}", i + 1),
                confidence: Confidence::Low,
                zone: LedZone::Button,
            });
        }

        PredictedLayout {
            product_name: "Test Device".into(),
            total_leds,
            input_count,
            output_halo_segments: 11,
            first_button_index: button_start,
            button_count: 13,
            leds,
        }
    }
}
