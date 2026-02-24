//! CLI subcommands — device info, LED control, mute monitoring.

mod config_cmd;
mod descriptor;
mod devices;
mod map;
mod monitor;
mod mute;
mod predict;
mod probe;
mod status;

use clap::Subcommand;
use serde::Serialize;

pub(super) use crate::RUNNING;
pub(super) use focusmute_lib::audio::{self, MuteMonitor};
pub(super) use focusmute_lib::config::Config;
pub(super) use focusmute_lib::context::DeviceContext;
pub(super) use focusmute_lib::device::{self, DiscoveredDevice, ScarlettDevice, open_device};
pub(super) use focusmute_lib::error::Result;
pub(super) use focusmute_lib::layout;
pub(super) use focusmute_lib::led;
pub(super) use focusmute_lib::models;
pub(super) use focusmute_lib::monitor::{MonitorAction, MuteIndicator};
pub(super) use focusmute_lib::reconnect::ReconnectState;
pub(super) use focusmute_lib::schema;

const PADDING: usize = 2;

/// Compute alignment width for a command's key-value output.
/// Ensures at least PADDING spaces after the longest key in either level,
/// with top-level and indent values aligned to the same column.
pub(super) fn kv_width(top: &[&str], indent: &[&str]) -> usize {
    let top_max = top.iter().map(|k| k.len()).max().unwrap_or(0);
    let indent_max = indent.iter().map(|k| k.len()).max().unwrap_or(0);
    let top_need = if top.is_empty() { 0 } else { top_max + PADDING };
    // Indent keys lose 2 chars of inner width to the "  " prefix
    let indent_need = if indent.is_empty() {
        0
    } else {
        indent_max + PADDING + 2
    };
    top_need.max(indent_need)
}

pub(super) fn format_kv(key: &str, value: impl std::fmt::Display, w: usize) -> String {
    format!("{key:<width$}{value}", width = w)
}

pub(super) fn kv(key: &str, value: impl std::fmt::Display, w: usize) {
    println!("{key:<width$}{value}", width = w);
}

pub(super) fn kv_indent(key: &str, value: impl std::fmt::Display, w: usize) {
    println!("  {key:<width$}{value}", width = w - 2);
}

// ── JSON output structs ──

#[derive(Serialize)]
pub(super) struct StatusOutput {
    pub version: String,
    pub device: Option<DeviceStatusJson>,
    pub microphone: Option<MicrophoneStatusJson>,
    pub config: ConfigSummaryJson,
}

#[derive(Serialize)]
pub(super) struct DeviceStatusJson {
    pub model: String,
    pub firmware: String,
    pub serial: Option<String>,
    pub path: String,
    pub led_support: Option<String>,
}

#[derive(Serialize)]
pub(super) struct MicrophoneStatusJson {
    pub muted: bool,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub(super) struct ConfigSummaryJson {
    pub mute_color: String,
    pub mute_inputs: String,
    pub hotkey: String,
    pub sound_enabled: bool,
    pub autostart: bool,
}

#[derive(Serialize)]
pub(super) struct ConfigOutput {
    pub config_file: Option<String>,
    pub config_file_exists: bool,
    pub settings: Config,
    pub files: ConfigFilesJson,
}

#[derive(Serialize)]
pub(super) struct ConfigFilesJson {
    pub schema_cache: Option<String>,
    pub schema_cache_exists: bool,
}

#[derive(Serialize)]
pub(super) struct DevicesOutput {
    pub count: usize,
    pub devices: Vec<DiscoveredDevice>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Dump raw descriptor bytes
    Descriptor {
        /// Byte offset to read from (default: 0)
        #[arg(long, default_value_t = 0)]
        offset: u32,
        /// Number of bytes to read (default: 720)
        #[arg(long, default_value_t = focusmute_lib::protocol::DESCRIPTOR_SIZE)]
        size: u32,
    },

    /// Run mute indicator (monitors mic mute, changes LED color)
    Monitor,

    /// Map directLEDValues — lights one index at a time to identify LEDs
    Map {
        /// LED brightness (0-255, used as grayscale color)
        #[arg(long, default_value_t = 255)]
        value: u8,
        /// Flash cycle duration in seconds (higher = slower flash)
        #[arg(long, default_value_t = 2)]
        delay: u64,
        /// Only test a single index
        #[arg(long)]
        index: Option<u8>,
        /// Number of LED indices to scan (default: auto-detected from schema, or 40)
        #[arg(long)]
        count: Option<usize>,
        /// Save mapping results as JSON
        #[arg(long)]
        output: Option<String>,
        /// Print a Rust ModelProfile code snippet after mapping
        #[arg(long)]
        output_code: bool,
        /// Skip the LED-state warning and confirmation prompt
        #[arg(long)]
        accept: bool,
    },

    /// Probe device capabilities and extract firmware schema
    Probe {
        /// Dump full schema JSON to stdout
        #[arg(long)]
        dump_schema: bool,
    },

    /// Predict LED layout from a schema JSON file (no hardware required)
    Predict {
        /// Path to schema JSON file (from `probe --dump-schema > schema.json`)
        schema_file: String,
    },

    /// Show current configuration and file paths
    Config,

    /// Show device and microphone status
    Status,

    /// Mute the default capture device
    Mute,

    /// Unmute the default capture device
    Unmute,

    /// List connected Focusrite devices
    Devices,
}

/// Warn if `--json` was passed to a command that doesn't support it.
fn warn_json_unsupported(cmd_name: &str) {
    log::warn!("--json is not supported for `{cmd_name}` (ignored)");
}

pub fn run(cmd: Command, json: bool) -> Result<()> {
    match cmd {
        Command::Descriptor { offset, size } => {
            if json {
                warn_json_unsupported("descriptor");
            }
            descriptor::cmd_descriptor(offset, size)
        }
        Command::Monitor => {
            if json {
                warn_json_unsupported("monitor");
            }
            monitor::cmd_monitor()
        }
        Command::Map {
            value,
            delay,
            index,
            count,
            output,
            output_code,
            accept,
        } => {
            if json {
                warn_json_unsupported("map");
            }
            map::cmd_map(value, delay, index, count, output, output_code, accept)
        }
        Command::Probe { dump_schema } => {
            if json {
                warn_json_unsupported("probe");
            }
            probe::cmd_probe(dump_schema)
        }
        Command::Predict { schema_file } => predict::cmd_predict(schema_file, json),
        Command::Config => config_cmd::cmd_config(json),
        Command::Status => status::cmd_status(json),
        Command::Mute => {
            if json {
                warn_json_unsupported("mute");
            }
            mute::cmd_set_mute(mute::MuteAction::Mute)
        }
        Command::Unmute => {
            if json {
                warn_json_unsupported("unmute");
            }
            mute::cmd_set_mute(mute::MuteAction::Unmute)
        }
        Command::Devices => devices::cmd_devices(json),
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn kv_width_top_only() {
        let w = kv_width(&["Short:", "Longer key:"], &[]);
        // "Longer key:" = 11 + PADDING = 13
        assert_eq!(w, 13);
    }

    #[test]
    fn kv_width_indent_drives_width() {
        // Indent key needs +2 for the prefix
        let w = kv_width(&["A:"], &["Very long indent key:"]);
        // "Very long indent key:" = 21 + PADDING + 2 = 25
        assert_eq!(w, 25);
    }

    #[test]
    fn kv_width_top_drives_width() {
        let w = kv_width(&["Very long top key:"], &["Short:"]);
        // top: 18+2=20, indent: 6+2+2=10 → 20
        assert_eq!(w, 20);
    }

    #[test]
    fn values_align_across_levels() {
        let w = kv_width(&["Top:"], &["Indent:"]);
        let top = format_kv("Top:", "V", w);
        // Simulate kv_indent output
        let indent = format!("  {:<width$}{}", "Indent:", "V", width = w - 2);
        assert_eq!(top.find('V'), indent.find('V'));
    }

    #[test]
    fn status_width_is_compact() {
        // status command should have a tight width, not inflated by probe/config keys
        let w = kv_width(
            &["Version:", "Device:", "Microphone:"],
            &[
                "Model:",
                "Firmware:",
                "Serial:",
                "Path:",
                "LED support:",
                "Name:",
                "Mute color:",
                "Mute inputs:",
                "Hotkey:",
                "Sound:",
                "Autostart:",
            ],
        );
        // Longest indent key: "LED support:" (12) → 12 + 2 + 2 = 16
        assert_eq!(w, 16);
    }
}

#[cfg(test)]
mod format_kv_tests {
    use super::*;

    #[test]
    fn format_kv_basic() {
        let result = format_kv("Key:", "value", 10);
        // "Key:" is 4 chars, padded to 10, then "value"
        assert_eq!(result, "Key:      value");
    }

    #[test]
    fn format_kv_exact_width() {
        let result = format_kv("ExactWidth:", "val", 10);
        // "ExactWidth:" is 11 chars — exceeds width, no padding added
        assert_eq!(result, "ExactWidth:val");
    }

    #[test]
    fn kv_width_empty_both() {
        let w = kv_width(&[], &[]);
        assert_eq!(w, 0);
    }
}

#[cfg(test)]
mod json_struct_tests {
    use super::*;

    #[test]
    fn config_summary_json_has_expected_fields() {
        let summary = ConfigSummaryJson {
            mute_color: "#FF0000".into(),
            mute_inputs: "all".into(),
            hotkey: "Ctrl+Shift+M".into(),
            sound_enabled: true,
            autostart: false,
        };
        let json = serde_json::to_value(&summary).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 5, "ConfigSummaryJson should have 5 fields");
        assert!(obj.contains_key("mute_color"));
        assert!(obj.contains_key("mute_inputs"));
        assert!(obj.contains_key("hotkey"));
        assert!(obj.contains_key("sound_enabled"));
        assert!(obj.contains_key("autostart"));
    }

    #[test]
    fn status_output_has_expected_fields() {
        let output = StatusOutput {
            version: "0.1.0".into(),
            device: None,
            microphone: None,
            config: ConfigSummaryJson {
                mute_color: "#FF0000".into(),
                mute_inputs: "all".into(),
                hotkey: "Ctrl+Shift+M".into(),
                sound_enabled: true,
                autostart: false,
            },
        };
        let json = serde_json::to_value(&output).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 4, "StatusOutput should have 4 fields");
    }

    #[test]
    fn device_status_json_has_expected_fields() {
        let dev = DeviceStatusJson {
            model: "Test".into(),
            firmware: "1.0".into(),
            serial: None,
            path: "test://".into(),
            led_support: None,
        };
        let json = serde_json::to_value(&dev).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.len(), 5, "DeviceStatusJson should have 5 fields");
    }
}

#[cfg(test)]
mod json_output_tests {
    use super::*;

    #[test]
    fn status_output_with_null_device() {
        let output = StatusOutput {
            version: "0.1.0".into(),
            device: None,
            microphone: None,
            config: ConfigSummaryJson {
                mute_color: "#FF0000 (red)".into(),
                mute_inputs: "all".into(),
                hotkey: "Ctrl+Shift+M".into(),
                sound_enabled: true,
                autostart: false,
            },
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], "0.1.0");
        assert!(parsed["device"].is_null());
        assert!(parsed["microphone"].is_null());
        assert_eq!(parsed["config"]["sound_enabled"], true);
    }

    #[test]
    fn status_output_with_device_and_mic() {
        let output = StatusOutput {
            version: "0.1.0".into(),
            device: Some(DeviceStatusJson {
                model: "Scarlett 2i2 4th Gen".into(),
                firmware: "2.0.2417.0".into(),
                serial: Some("ABC123".into()),
                path: "test://path".into(),
                led_support: Some("hardcoded (2 inputs, 40 LEDs)".into()),
            }),
            microphone: Some(MicrophoneStatusJson {
                muted: true,
                name: Some("Test Mic".into()),
            }),
            config: ConfigSummaryJson {
                mute_color: "#FF0000 (red)".into(),
                mute_inputs: "all".into(),
                hotkey: "Ctrl+Shift+M".into(),
                sound_enabled: true,
                autostart: false,
            },
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["device"]["model"], "Scarlett 2i2 4th Gen");
        assert_eq!(parsed["device"]["serial"], "ABC123");
        assert_eq!(parsed["microphone"]["muted"], true);
        assert_eq!(parsed["microphone"]["name"], "Test Mic");
    }

    #[test]
    fn config_output_complete() {
        let output = ConfigOutput {
            config_file: Some("/home/user/.config/focusmute/config.toml".into()),
            config_file_exists: true,
            settings: Config::default(),
            files: ConfigFilesJson {
                schema_cache: Some("/home/user/.config/focusmute/schema_cache.json".into()),
                schema_cache_exists: false,
            },
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // All top-level fields present
        assert!(parsed["config_file"].is_string());
        assert_eq!(parsed["config_file_exists"], true);
        assert!(parsed["settings"].is_object());
        assert!(parsed["files"].is_object());

        // Settings fields from Config
        assert_eq!(parsed["settings"]["mute_color"], "#FF0000");
        assert_eq!(parsed["settings"]["hotkey"], "Ctrl+Shift+M");
        assert_eq!(parsed["settings"]["sound_enabled"], true);
        assert_eq!(parsed["settings"]["autostart"], false);
        assert_eq!(parsed["settings"]["mute_inputs"], "all");

        // Files section
        assert!(parsed["files"]["schema_cache"].is_string());
        assert_eq!(parsed["files"]["schema_cache_exists"], false);
    }

    #[test]
    fn config_output_missing_paths_are_null() {
        let output = ConfigOutput {
            config_file: None,
            config_file_exists: false,
            settings: Config::default(),
            files: ConfigFilesJson {
                schema_cache: None,
                schema_cache_exists: false,
            },
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["config_file"].is_null());
        assert!(parsed["files"]["schema_cache"].is_null());
    }

    #[test]
    fn devices_output_empty() {
        let output = DevicesOutput {
            count: 0,
            devices: vec![],
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["count"], 0);
        assert!(parsed["devices"].as_array().unwrap().is_empty());
    }

    #[test]
    fn devices_output_with_devices() {
        let output = DevicesOutput {
            count: 2,
            devices: vec![
                DiscoveredDevice {
                    path: "usb:001/002".into(),
                    serial: Some("SERIAL1".into()),
                },
                DiscoveredDevice {
                    path: "usb:001/003".into(),
                    serial: None,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["count"], 2);
        let devices = parsed["devices"].as_array().unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0]["serial"], "SERIAL1");
        assert!(devices[1]["serial"].is_null());
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    /// Minimal valid schema JSON for cmd_predict testing.
    fn test_schema_json() -> String {
        serde_json::json!({
            "device-specification": {
                "product-name": "Scarlett 2i2 4th Gen"
            },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string()
    }

    #[test]
    fn cmd_predict_valid_schema_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        std::fs::write(&path, test_schema_json()).unwrap();

        let result = predict::cmd_predict(path.to_str().unwrap().to_string(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_predict_json_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        std::fs::write(&path, test_schema_json()).unwrap();

        let result = predict::cmd_predict(path.to_str().unwrap().to_string(), true);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_predict_missing_file_returns_error() {
        let result = predict::cmd_predict("/nonexistent/schema.json".to_string(), false);
        assert!(result.is_err());
    }

    #[test]
    fn cmd_predict_invalid_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();

        let result = predict::cmd_predict(path.to_str().unwrap().to_string(), false);
        assert!(result.is_err());
    }

    #[test]
    fn cmd_predict_empty_schema_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "{}").unwrap();

        let result = predict::cmd_predict(path.to_str().unwrap().to_string(), false);
        // Empty schema has no LEDs, so predict_layout should error
        assert!(result.is_err());
    }

    #[test]
    fn cmd_config_succeeds() {
        // cmd_config reads the config (or defaults) and prints it.
        // Should never fail even without a config file.
        let result = config_cmd::cmd_config(false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_config_json_succeeds() {
        let result = config_cmd::cmd_config(true);
        assert!(result.is_ok());
    }
}
