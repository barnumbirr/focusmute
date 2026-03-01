//! Settings dialog — cross-platform egui UI for editing configuration.
//!
//! Shows a modal dialog allowing the user to edit:
//! - Mute color (hex or name)
//! - Hotkey
//! - Mute inputs (dropdown: All, Input 1, Input 2, Input 1+2)
//! - Sound feedback (checkbox)
//! - Custom mute/unmute sounds
//! - Autostart (checkbox)
//!
//! Returns `Some(new_config)` on Save, `None` on Cancel/close.

use focusmute_lib::config::Config;
use focusmute_lib::device::DeviceInfo;
use focusmute_lib::models::ModelProfile;

#[cfg(any(windows, target_os = "linux"))]
mod ui;

/// Maximum custom sound file size (10 MB).
const MAX_SOUND_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Holds a rodio output stream and sink for previewing sounds in the settings dialog.
#[cfg(any(windows, target_os = "linux"))]
pub(crate) struct SoundPreviewPlayer {
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
}

#[cfg(any(windows, target_os = "linux"))]
impl SoundPreviewPlayer {
    pub fn try_new() -> Option<Self> {
        let (stream, sink) = crate::sound::init_audio_output();
        Some(SoundPreviewPlayer {
            _stream: stream?,
            sink: sink?,
        })
    }

    pub fn play(&self, path: &str, fallback: &'static [u8]) {
        // Stop any currently playing preview
        self.sink.stop();
        let sound = crate::sound::load_sound_data(path, fallback);
        crate::sound::play_sound(&sound, &self.sink);
    }
}

/// Build the mute_inputs dropdown items and find the selected index.
pub(crate) fn inputs_combo_items(config: &Config, input_count: usize) -> (Vec<String>, usize) {
    let mut items = vec!["All".to_string()];
    for i in 1..=input_count {
        items.push(format!("Input {i}"));
    }
    if input_count >= 2 {
        let all_nums: Vec<String> = (1..=input_count).map(|i| i.to_string()).collect();
        items.push(format!("Input {}", all_nums.join("+")));
    }

    let mode = config.parse_mute_inputs();
    let selected = match mode {
        focusmute_lib::config::MuteInputs::All => 0,
        focusmute_lib::config::MuteInputs::Specific(ref inputs) => {
            if input_count >= 2 && inputs.len() == input_count {
                items.len() - 1
            } else if inputs.len() == 1 {
                inputs[0] + 1
            } else {
                0
            }
        }
    };
    let selected = selected.min(items.len() - 1);
    (items, selected)
}

/// Convert combo selection index back to mute_inputs string.
pub(crate) fn combo_to_mute_inputs(index: usize, input_count: usize) -> String {
    if index == 0 {
        return "all".to_string();
    }
    if input_count >= 2 && index == input_count + 1 {
        let nums: Vec<String> = (1..=input_count).map(|i| i.to_string()).collect();
        return nums.join(",");
    }
    format!("{index}")
}

/// Show the settings dialog and return the new config if the user clicks Save.
///
/// This is modal — blocks the calling thread until the dialog is closed.
///
/// Must be called from the main thread (eframe/winit requirement).
pub fn show_settings(
    config: &Config,
    model: Option<&ModelProfile>,
    device_info: Option<&DeviceInfo>,
) -> Option<Config> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        use std::sync::{Arc, Mutex};

        let config_clone = config.clone();
        let input_count = model.map_or(0, |m| m.input_count);

        let mut device_lines: Vec<(String, String)> = Vec::new();
        if let Some(info) = device_info {
            device_lines.push(("Device".into(), info.model().to_string()));
            device_lines.push(("Firmware".into(), info.firmware.to_string()));
            if let Some(ref serial) = info.serial {
                device_lines.push(("Serial".into(), serial.clone()));
            }
        } else {
            device_lines.push(("Device".into(), "not connected".into()));
        }

        let result: Arc<Mutex<Option<Config>>> = Arc::new(Mutex::new(None));
        let result_for_app = result.clone();

        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([440.0, 390.0])
                .with_resizable(false)
                .with_title("FocusMute Settings")
                .with_icon(crate::icon::app_icon()),
            ..Default::default()
        };
        if let Err(e) = eframe::run_native(
            "FocusMute Settings",
            options,
            Box::new(move |cc| {
                Ok(Box::new(ui::SettingsApp::new(
                    config_clone,
                    input_count,
                    device_lines,
                    result_for_app,
                    cc,
                )))
            }),
        ) {
            log::error!("settings dialog failed: {e}");
        }

        // Extract result after run_native returns (window closed)
        result.lock().ok().and_then(|mut guard| guard.take())
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (config, model, device_info);
        log::warn!("Settings dialog is not available on this platform.");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focusmute_lib::config::MuteInputs;

    // ── inputs_combo_items ──

    #[test]
    fn combo_items_zero_inputs() {
        let c = Config::default();
        let (items, sel) = inputs_combo_items(&c, 0);
        assert_eq!(items, vec!["All"]);
        assert_eq!(sel, 0);
    }

    #[test]
    fn combo_items_one_input() {
        let c = Config::default();
        let (items, sel) = inputs_combo_items(&c, 1);
        assert_eq!(items, vec!["All", "Input 1"]);
        assert_eq!(sel, 0);
    }

    #[test]
    fn combo_items_two_inputs() {
        let c = Config::default();
        let (items, sel) = inputs_combo_items(&c, 2);
        assert_eq!(items, vec!["All", "Input 1", "Input 2", "Input 1+2"]);
        assert_eq!(sel, 0);
    }

    #[test]
    fn combo_items_specific_input_selected() {
        let c = Config {
            mute_inputs: "1".into(),
            ..Config::default()
        };
        let (items, sel) = inputs_combo_items(&c, 2);
        assert_eq!(items.len(), 4);
        assert_eq!(sel, 1); // "Input 1" is index 1
    }

    #[test]
    fn combo_items_second_input_selected() {
        let c = Config {
            mute_inputs: "2".into(),
            ..Config::default()
        };
        let (_, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, 2); // "Input 2" is index 2
    }

    #[test]
    fn combo_items_all_combined_selected() {
        let c = Config {
            mute_inputs: "1,2".into(),
            ..Config::default()
        };
        let (items, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, items.len() - 1); // "Input 1+2" is the last item
    }

    #[test]
    fn combo_items_all_string_selects_first() {
        let c = Config {
            mute_inputs: "all".into(),
            ..Config::default()
        };
        let (_, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, 0);
    }

    // ── combo_to_mute_inputs ──

    #[test]
    fn combo_to_mute_all() {
        assert_eq!(combo_to_mute_inputs(0, 2), "all");
    }

    #[test]
    fn combo_to_mute_input_1() {
        assert_eq!(combo_to_mute_inputs(1, 2), "1");
    }

    #[test]
    fn combo_to_mute_input_2() {
        assert_eq!(combo_to_mute_inputs(2, 2), "2");
    }

    #[test]
    fn combo_to_mute_all_combined() {
        // For 2 inputs, index 3 (= input_count + 1) is "Input 1+2"
        assert_eq!(combo_to_mute_inputs(3, 2), "1,2");
    }

    #[test]
    fn combo_to_mute_single_input_device() {
        // For 1 input, index 0 = "all", index 1 = "1"
        assert_eq!(combo_to_mute_inputs(0, 1), "all");
        assert_eq!(combo_to_mute_inputs(1, 1), "1");
    }

    // ── round-trip: combo index ↔ mute_inputs string ──

    #[test]
    fn roundtrip_all() {
        let mute_str = combo_to_mute_inputs(0, 2);
        let c = Config {
            mute_inputs: mute_str,
            ..Config::default()
        };
        let (_, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, 0);
    }

    #[test]
    fn roundtrip_specific_input() {
        let mute_str = combo_to_mute_inputs(1, 2);
        let c = Config {
            mute_inputs: mute_str.clone(),
            ..Config::default()
        };
        let parsed = c.parse_mute_inputs();
        assert_eq!(parsed, MuteInputs::Specific(vec![0]));
        let (_, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, 1);
    }

    #[test]
    fn roundtrip_all_combined() {
        let mute_str = combo_to_mute_inputs(3, 2);
        let c = Config {
            mute_inputs: mute_str,
            ..Config::default()
        };
        let (items, sel) = inputs_combo_items(&c, 2);
        assert_eq!(sel, items.len() - 1);
    }
}
