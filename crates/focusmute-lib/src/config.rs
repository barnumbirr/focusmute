//! Application configuration — TOML-based, platform-aware paths.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

/// Header comment prepended to saved config files.
const CONFIG_HEADER: &str =
    "# FocusMute configuration — changes made outside the app may be overwritten.\n\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Mute indicator color (hex or name). Default: "#FF0000" (red).
    #[serde(default = "default_mute_color")]
    pub mute_color: String,

    /// Global hotkey to toggle mute. Default: "Ctrl+Shift+M". Format: "Modifier+Key" or just "Key".
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Play a sound when mute state changes.
    #[serde(default = "default_true")]
    pub sound_enabled: bool,

    /// Start application on login.
    #[serde(default)]
    pub autostart: bool,

    /// Which inputs to show mute indicator on. Default: "all".
    /// Values: "all", "1", "2", "1,2", etc. (1-based input numbers).
    #[serde(default = "default_mute_inputs")]
    pub mute_inputs: String,

    /// Path to custom mute sound WAV file. Empty = use built-in.
    #[serde(default)]
    pub mute_sound_path: String,

    /// Path to custom unmute sound WAV file. Empty = use built-in.
    #[serde(default)]
    pub unmute_sound_path: String,

    /// Preferred device serial number. Empty = auto-select first device.
    #[serde(default)]
    pub device_serial: String,

    /// Command to run when muted. Empty = disabled.
    #[serde(default)]
    pub on_mute_command: String,

    /// Command to run when unmuted. Empty = disabled.
    #[serde(default)]
    pub on_unmute_command: String,

    /// Per-input mute colors (1-based keys). Overrides `mute_color` for specific inputs.
    /// Example in TOML: `[input_colors]` / `1 = "#FF0000"` / `2 = "#0000FF"`
    #[serde(default)]
    pub input_colors: HashMap<String, String>,

    /// Show desktop notification on mute state change.
    #[serde(default)]
    pub notifications_enabled: bool,
}

fn default_mute_color() -> String {
    "#FF0000".into()
}
fn default_hotkey() -> String {
    "Ctrl+Shift+M".into()
}
fn default_mute_inputs() -> String {
    "all".into()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mute_color: default_mute_color(),
            hotkey: default_hotkey(),
            sound_enabled: true,
            autostart: false,
            mute_inputs: default_mute_inputs(),
            mute_sound_path: String::new(),
            unmute_sound_path: String::new(),
            device_serial: String::new(),
            on_mute_command: String::new(),
            on_unmute_command: String::new(),
            input_colors: HashMap::new(),
            notifications_enabled: false,
        }
    }
}

/// Parsed mute input selection.
#[derive(Debug, Clone, PartialEq)]
pub enum MuteInputs {
    /// All inputs.
    All,
    /// Specific inputs (0-indexed internally, parsed from 1-based user input).
    Specific(Vec<usize>),
}

impl std::fmt::Display for MuteInputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MuteInputs::All => write!(f, "all"),
            MuteInputs::Specific(inputs) => {
                let names: Vec<String> = inputs.iter().map(|i| format!("{}", i + 1)).collect();
                write!(f, "{} (per-input)", names.join(", "))
            }
        }
    }
}

/// Validation errors that [`Config::validate`] can return.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// The `mute_color` field could not be parsed as a valid color.
    InvalidColor(String),
    /// The `hotkey` field is empty or whitespace-only.
    EmptyHotkey,
    /// A custom sound path is invalid (`field` is `"mute_sound_path"` or `"unmute_sound_path"`).
    InvalidSoundPath { field: &'static str, reason: String },
    /// The `mute_inputs` field references inputs that don't exist on the device.
    InvalidMuteInputs(String),
    /// An `input_colors` entry is invalid (bad color value or out-of-range key).
    InvalidInputColor { input: String, reason: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidColor(e) => write!(f, "Invalid mute color: {e}"),
            ValidationError::EmptyHotkey => write!(f, "Hotkey cannot be empty"),
            ValidationError::InvalidSoundPath { field, reason } => {
                write!(f, "Invalid {field}: {reason}")
            }
            ValidationError::InvalidMuteInputs(e) => write!(f, "Invalid mute inputs: {e}"),
            ValidationError::InvalidInputColor { input, reason } => {
                write!(f, "Invalid input_colors[{input}]: {reason}")
            }
        }
    }
}

impl Config {
    /// Platform-specific config directory.
    pub fn dir() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            dirs::config_dir().map(|p| p.join("Focusmute"))
        }
        #[cfg(not(windows))]
        {
            dirs::config_dir().map(|p| p.join("focusmute"))
        }
    }

    /// Full path to config file.
    pub fn path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("config.toml"))
    }

    /// Full path to the log file (tray app).
    pub fn log_path() -> Option<PathBuf> {
        Self::dir().map(|d| d.join("focusmute.log"))
    }

    /// Load config from disk, or return defaults if not found.
    pub fn load() -> Self {
        let (config, warnings) = Self::load_with_warnings();
        for w in &warnings {
            log::warn!("{w}");
        }
        config
    }

    /// Save config to an arbitrary path atomically (write to temp file, then rename).
    ///
    /// A header comment is prepended to warn that manual edits may be overwritten.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let serialized = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let contents = format!("{CONFIG_HEADER}{serialized}");
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &contents)?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Rename can fail across filesystems; fall back to direct write + cleanup
                let result = std::fs::write(path, &contents);
                let _ = std::fs::remove_file(&tmp);
                result
            }
        }
    }

    /// Save config to the default platform path.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No config directory",
            ));
        };
        self.save_to(&path)
    }

    /// Load config from an arbitrary path, returning the config and any parse warnings.
    ///
    /// Returns `(defaults, [])` if the file doesn't exist.
    /// Returns `(defaults, [warning])` if the file exists but can't be parsed.
    pub fn load_from(path: &Path) -> (Self, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => (config, vec![]),
                Err(e) => {
                    let warning = format!(
                        "config parse error ({}), using defaults: {e}",
                        path.display()
                    );
                    (Self::default(), vec![warning])
                }
            },
            Err(_) => (Self::default(), vec![]),
        }
    }

    /// Load config from the default path, returning the config and any parse warnings.
    pub fn load_with_warnings() -> (Self, Vec<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), vec![]);
        };
        Self::load_from(&path)
    }

    /// Parse the `mute_inputs` field into a `MuteInputs` enum.
    ///
    /// - `"all"` → `MuteInputs::All`
    /// - `"1"` → `MuteInputs::Specific(vec![0])`  (1-based → 0-indexed)
    /// - `"1,2"` → `MuteInputs::Specific(vec![0, 1])`
    ///
    /// Returns `MuteInputs::All` for empty or unparseable values.
    pub fn parse_mute_inputs(&self) -> MuteInputs {
        let s = self.mute_inputs.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("all") {
            return MuteInputs::All;
        }
        let mut inputs = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if let Ok(n) = part.parse::<usize>()
                && n >= 1
            {
                let idx = n - 1; // convert 1-based to 0-indexed
                if !inputs.contains(&idx) {
                    inputs.push(idx);
                }
            }
        }
        if inputs.is_empty() {
            MuteInputs::All
        } else {
            inputs.sort();
            MuteInputs::Specific(inputs)
        }
    }

    /// Validate a sound file path. Empty = built-in (always Ok).
    /// Checks: file exists, .wav extension, size <= max_size_bytes.
    pub fn validate_sound_path(path: &str, max_size_bytes: u64) -> crate::error::Result<()> {
        let path = path.trim();
        if path.is_empty() {
            return Ok(());
        }
        let p = std::path::Path::new(path);
        if !p.exists() {
            return Err(crate::FocusmuteError::Config(format!(
                "File not found: {path}"
            )));
        }
        match p.extension().and_then(|e| e.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("wav") => {}
            _ => {
                return Err(crate::FocusmuteError::Config(format!(
                    "Not a .wav file: {path}"
                )));
            }
        }
        match std::fs::metadata(p) {
            Ok(meta) => {
                if meta.len() > max_size_bytes {
                    return Err(crate::FocusmuteError::Config(format!(
                        "File too large: {} bytes (max {})",
                        meta.len(),
                        max_size_bytes
                    )));
                }
            }
            Err(e) => {
                return Err(crate::FocusmuteError::Config(format!(
                    "Cannot read file: {e}"
                )));
            }
        }
        Ok(())
    }

    /// Validate the entire config, collecting all errors.
    ///
    /// - `input_count`: if `Some`, validates `mute_inputs` against the device's input count.
    /// - `max_sound_bytes`: maximum allowed size for custom sound files.
    ///
    /// Returns `Ok(())` if valid, or `Err(Vec<ValidationError>)` with all problems found.
    pub fn validate(
        &self,
        input_count: Option<usize>,
        max_sound_bytes: u64,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Validate color
        if let Err(e) = crate::led::parse_color(&self.mute_color) {
            errors.push(ValidationError::InvalidColor(e.to_string()));
        }

        // Validate hotkey
        if self.hotkey.trim().is_empty() {
            errors.push(ValidationError::EmptyHotkey);
        }

        // Validate sound paths
        if let Err(e) = Self::validate_sound_path(&self.mute_sound_path, max_sound_bytes) {
            errors.push(ValidationError::InvalidSoundPath {
                field: "mute_sound_path",
                reason: e.to_string(),
            });
        }
        if let Err(e) = Self::validate_sound_path(&self.unmute_sound_path, max_sound_bytes) {
            errors.push(ValidationError::InvalidSoundPath {
                field: "unmute_sound_path",
                reason: e.to_string(),
            });
        }

        // Validate mute inputs if input count is known
        if let Some(count) = input_count
            && let Err(e) = self.validate_mute_inputs(count)
        {
            errors.push(ValidationError::InvalidMuteInputs(e.to_string()));
        }

        // Validate input_colors entries
        for (key, value) in &self.input_colors {
            if let Err(e) = crate::led::parse_color(value) {
                errors.push(ValidationError::InvalidInputColor {
                    input: key.clone(),
                    reason: e.to_string(),
                });
            }
            if let Some(count) = input_count {
                match key.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= count => {}
                    Ok(n) => {
                        errors.push(ValidationError::InvalidInputColor {
                            input: key.clone(),
                            reason: format!(
                                "input {n} is out of range (device has {count} input{})",
                                if count == 1 { "" } else { "s" }
                            ),
                        });
                    }
                    Err(_) => {
                        errors.push(ValidationError::InvalidInputColor {
                            input: key.clone(),
                            reason: format!("key must be a 1-based input number, got \"{key}\""),
                        });
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate mute_inputs against a model's input count.
    /// Returns an error if any input number exceeds the model's capacity.
    pub fn validate_mute_inputs(&self, input_count: usize) -> crate::error::Result<()> {
        match self.parse_mute_inputs() {
            MuteInputs::All => Ok(()),
            MuteInputs::Specific(inputs) => {
                for &idx in &inputs {
                    if idx >= input_count {
                        return Err(crate::FocusmuteError::Config(format!(
                            "Input {} is out of range (device has {} input{})",
                            idx + 1,
                            input_count,
                            if input_count == 1 { "" } else { "s" }
                        )));
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Display impl ──

    #[test]
    fn display_mute_inputs_all() {
        assert_eq!(MuteInputs::All.to_string(), "all");
    }

    #[test]
    fn display_mute_inputs_specific() {
        let inputs = MuteInputs::Specific(vec![0, 1]);
        assert_eq!(inputs.to_string(), "1, 2 (per-input)");
    }

    #[test]
    fn display_mute_inputs_single() {
        let inputs = MuteInputs::Specific(vec![0]);
        assert_eq!(inputs.to_string(), "1 (per-input)");
    }

    // ── Config defaults ──

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!(c.mute_color, "#FF0000");
        assert_eq!(c.hotkey, "Ctrl+Shift+M");
        assert!(c.sound_enabled);
        assert!(!c.autostart);
        assert_eq!(c.mute_inputs, "all");
    }

    #[test]
    fn serialize_roundtrip() {
        let c = Config {
            mute_color: "#00FF00".into(),
            hotkey: "F12".into(),
            sound_enabled: false,
            autostart: true,
            mute_inputs: "1,2".into(),
            ..Config::default()
        };
        let toml_str = toml::to_string_pretty(&c).unwrap();
        let c2: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(c2.mute_color, "#00FF00");
        assert_eq!(c2.hotkey, "F12");
        assert!(!c2.sound_enabled);
        assert!(c2.autostart);
        assert_eq!(c2.mute_inputs, "1,2");
    }

    #[test]
    fn partial_toml_fills_defaults() {
        let toml_str = "mute_color = \"#0000FF\"";
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.mute_color, "#0000FF");
        // Missing fields get defaults
        assert_eq!(c.hotkey, "Ctrl+Shift+M");
        assert!(c.sound_enabled);
        assert!(!c.autostart);
        assert_eq!(c.mute_inputs, "all");
    }

    #[test]
    fn empty_toml_gives_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.mute_color, "#FF0000");
        assert_eq!(c.hotkey, "Ctrl+Shift+M");
        assert!(c.sound_enabled);
        assert!(!c.autostart);
        assert_eq!(c.mute_inputs, "all");
    }

    #[test]
    fn malformed_toml_gives_defaults() {
        // toml::from_str returns Err for malformed input — Config::load would
        // use defaults with a warning. Test that the fallback behavior is correct.
        let result: std::result::Result<Config, _> = toml::from_str("this is { not valid toml");
        assert!(result.is_err());
        // After error, the app falls back to defaults
        let fallback = Config::default();
        assert_eq!(fallback.mute_color, "#FF0000");
        assert_eq!(fallback.hotkey, "Ctrl+Shift+M");
    }

    #[test]
    fn wrong_type_toml_gives_defaults() {
        // A valid TOML key with the wrong type (string where bool expected)
        let result: std::result::Result<Config, _> =
            toml::from_str("sound_enabled = \"not a bool\"");
        assert!(result.is_err());
    }

    #[test]
    fn config_path_is_some() {
        // Should always resolve on any platform with a home dir
        assert!(Config::dir().is_some());
        assert!(Config::path().is_some());
    }

    #[test]
    fn config_path_ends_with_toml() {
        let path = Config::path().unwrap();
        assert_eq!(path.file_name().unwrap(), "config.toml");
    }

    #[test]
    fn log_path_is_in_config_dir() {
        let log = Config::log_path().unwrap();
        let dir = Config::dir().unwrap();
        assert_eq!(log.parent().unwrap(), dir);
        assert_eq!(log.file_name().unwrap(), "focusmute.log");
    }

    #[test]
    fn backward_compat_old_toml_without_mute_inputs() {
        // Simulates an old config file that doesn't have mute_inputs
        let toml_str = r##"
mute_color = "#FF0000"
hotkey = "Ctrl+Shift+M"
sound_enabled = true
autostart = false
"##;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.mute_inputs, "all");
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    // ── parse_mute_inputs ──

    #[test]
    fn parse_mute_inputs_all() {
        let c = Config {
            mute_inputs: "all".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_all_case_insensitive() {
        let c = Config {
            mute_inputs: "ALL".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
        let c2 = Config {
            mute_inputs: "All".into(),
            ..Config::default()
        };
        assert_eq!(c2.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_empty_is_all() {
        let c = Config {
            mute_inputs: "".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_whitespace_is_all() {
        let c = Config {
            mute_inputs: "  ".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_single() {
        let c = Config {
            mute_inputs: "1".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0]));
    }

    #[test]
    fn parse_mute_inputs_single_input_2() {
        let c = Config {
            mute_inputs: "2".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![1]));
    }

    #[test]
    fn parse_mute_inputs_multiple() {
        let c = Config {
            mute_inputs: "1,2".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_with_spaces() {
        let c = Config {
            mute_inputs: " 1 , 2 ".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_deduplicates() {
        let c = Config {
            mute_inputs: "1,1,2,2".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_sorts() {
        let c = Config {
            mute_inputs: "2,1".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_zero_ignored() {
        // 0 is invalid (1-based), so it should be ignored
        let c = Config {
            mute_inputs: "0".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_garbage_is_all() {
        let c = Config {
            mute_inputs: "abc".into(),
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    // ── validate_mute_inputs ──

    #[test]
    fn validate_all_always_ok() {
        let c = Config {
            mute_inputs: "all".into(),
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(2).is_ok());
        assert!(c.validate_mute_inputs(0).is_ok());
    }

    #[test]
    fn validate_within_range() {
        let c = Config {
            mute_inputs: "1,2".into(),
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(2).is_ok());
    }

    #[test]
    fn validate_out_of_range() {
        let c = Config {
            mute_inputs: "3".into(),
            ..Config::default()
        };
        let err = c.validate_mute_inputs(2).unwrap_err();
        assert!(err.to_string().contains("out of range"), "got: {err}");
    }

    #[test]
    fn validate_single_input_device() {
        let c = Config {
            mute_inputs: "1".into(),
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(1).is_ok());
        let c2 = Config {
            mute_inputs: "2".into(),
            ..Config::default()
        };
        assert!(c2.validate_mute_inputs(1).is_err());
    }

    // ── sound path config ──

    #[test]
    fn defaults_include_empty_sound_paths() {
        let c = Config::default();
        assert!(c.mute_sound_path.is_empty());
        assert!(c.unmute_sound_path.is_empty());
    }

    #[test]
    fn serialize_roundtrip_with_sound_paths() {
        let c = Config {
            mute_sound_path: "C:\\sounds\\mute.wav".into(),
            unmute_sound_path: "C:\\sounds\\unmute.wav".into(),
            ..Config::default()
        };
        let toml_str = toml::to_string_pretty(&c).unwrap();
        let c2: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(c2.mute_sound_path, "C:\\sounds\\mute.wav");
        assert_eq!(c2.unmute_sound_path, "C:\\sounds\\unmute.wav");
    }

    #[test]
    fn backward_compat_old_toml_without_sound_paths() {
        let toml_str = r##"
mute_color = "#FF0000"
hotkey = "Ctrl+Shift+M"
sound_enabled = true
autostart = false
mute_inputs = "all"
"##;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert!(c.mute_sound_path.is_empty());
        assert!(c.unmute_sound_path.is_empty());
    }

    #[test]
    fn validate_sound_path_empty_is_ok() {
        assert!(Config::validate_sound_path("", 10_000_000).is_ok());
        assert!(Config::validate_sound_path("  ", 10_000_000).is_ok());
    }

    #[test]
    fn validate_sound_path_nonexistent_file() {
        let err = Config::validate_sound_path("C:\\no\\such\\file.wav", 10_000_000).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err}");
    }

    #[test]
    fn validate_sound_path_wrong_extension() {
        // Create a temp file with a non-.wav extension
        let dir = std::env::temp_dir();
        let path = dir.join("focusmute_test_sound.mp3");
        std::fs::write(&path, b"dummy").unwrap();
        let err = Config::validate_sound_path(path.to_str().unwrap(), 10_000_000).unwrap_err();
        assert!(err.to_string().contains("Not a .wav"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn validate_sound_path_file_too_large() {
        let dir = std::env::temp_dir();
        let path = dir.join("focusmute_test_large.wav");
        std::fs::write(&path, vec![0u8; 100]).unwrap();
        let err = Config::validate_sound_path(path.to_str().unwrap(), 50).unwrap_err();
        assert!(err.to_string().contains("too large"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn validate_sound_path_valid_wav() {
        let dir = std::env::temp_dir();
        let path = dir.join("focusmute_test_valid.wav");
        std::fs::write(&path, b"dummy wav content").unwrap();
        assert!(Config::validate_sound_path(path.to_str().unwrap(), 10_000_000).is_ok());
        let _ = std::fs::remove_file(&path);
    }

    // ── Config::validate() ──

    #[test]
    fn validate_default_config_ok() {
        let c = Config::default();
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn validate_default_config_with_input_count_ok() {
        let c = Config::default();
        assert!(c.validate(Some(2), 10_000_000).is_ok());
    }

    #[test]
    fn validate_invalid_color() {
        let c = Config {
            mute_color: "chartreuse".into(),
            ..Config::default()
        };
        let errs = c.validate(None, 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::InvalidColor(_)));
        assert!(errs[0].to_string().contains("Invalid mute color"));
    }

    #[test]
    fn validate_empty_hotkey() {
        let c = Config {
            hotkey: "  ".into(),
            ..Config::default()
        };
        let errs = c.validate(None, 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::EmptyHotkey));
        assert!(errs[0].to_string().contains("Hotkey cannot be empty"));
    }

    #[test]
    fn validate_bad_sound_path() {
        let c = Config {
            mute_sound_path: "/no/such/file.wav".into(),
            ..Config::default()
        };
        let errs = c.validate(None, 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ValidationError::InvalidSoundPath {
                field: "mute_sound_path",
                ..
            }
        ));
    }

    #[test]
    fn validate_bad_unmute_sound_path() {
        let c = Config {
            unmute_sound_path: "/no/such/file.wav".into(),
            ..Config::default()
        };
        let errs = c.validate(None, 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ValidationError::InvalidSoundPath {
                field: "unmute_sound_path",
                ..
            }
        ));
    }

    #[test]
    fn validate_bad_mute_inputs() {
        let c = Config {
            mute_inputs: "5".into(),
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::InvalidMuteInputs(_)));
    }

    #[test]
    fn validate_mute_inputs_skipped_without_count() {
        let c = Config {
            mute_inputs: "99".into(),
            ..Config::default()
        };
        // Without input_count, mute_inputs validation is skipped
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn validate_collects_multiple_errors() {
        let c = Config {
            mute_color: "not-a-color".into(),
            hotkey: "".into(),
            mute_sound_path: "/bad/path.wav".into(),
            unmute_sound_path: "/bad/path2.wav".into(),
            mute_inputs: "99".into(),
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 5);
        // Verify ordering: color, hotkey, mute_sound, unmute_sound, mute_inputs
        assert!(matches!(errs[0], ValidationError::InvalidColor(_)));
        assert!(matches!(errs[1], ValidationError::EmptyHotkey));
        assert!(matches!(
            &errs[2],
            ValidationError::InvalidSoundPath {
                field: "mute_sound_path",
                ..
            }
        ));
        assert!(matches!(
            &errs[3],
            ValidationError::InvalidSoundPath {
                field: "unmute_sound_path",
                ..
            }
        ));
        assert!(matches!(errs[4], ValidationError::InvalidMuteInputs(_)));
    }

    #[test]
    fn validate_valid_hex_color_and_named() {
        for color in &["#FF0000", "red", "blue", "#ABCDEF"] {
            let c = Config {
                mute_color: color.to_string(),
                ..Config::default()
            };
            assert!(c.validate(None, 10_000_000).is_ok(), "failed for {color}");
        }
    }

    #[test]
    fn validation_error_display() {
        let e = ValidationError::InvalidSoundPath {
            field: "mute_sound_path",
            reason: "file not found".into(),
        };
        assert_eq!(e.to_string(), "Invalid mute_sound_path: file not found");
    }

    #[test]
    fn config_round_trip_all_fields() {
        let config = Config {
            mute_color: "#00FF00".into(),
            hotkey: "Alt+M".into(),
            sound_enabled: false,
            autostart: true,
            mute_inputs: "1,2".into(),
            mute_sound_path: "/tmp/mute.wav".into(),
            unmute_sound_path: "/tmp/unmute.wav".into(),
            device_serial: "ABC123".into(),
            on_mute_command: "echo muted".into(),
            on_unmute_command: "echo unmuted".into(),
            input_colors: HashMap::from([
                ("1".into(), "#FF0000".into()),
                ("2".into(), "#0000FF".into()),
            ]),
            notifications_enabled: true,
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.mute_color, config.mute_color);
        assert_eq!(loaded.hotkey, config.hotkey);
        assert_eq!(loaded.sound_enabled, config.sound_enabled);
        assert_eq!(loaded.autostart, config.autostart);
        assert_eq!(loaded.mute_inputs, config.mute_inputs);
        assert_eq!(loaded.mute_sound_path, config.mute_sound_path);
        assert_eq!(loaded.unmute_sound_path, config.unmute_sound_path);
        assert_eq!(loaded.device_serial, config.device_serial);
        assert_eq!(loaded.on_mute_command, config.on_mute_command);
        assert_eq!(loaded.on_unmute_command, config.on_unmute_command);
        assert_eq!(loaded.input_colors, config.input_colors);
        assert_eq!(loaded.notifications_enabled, config.notifications_enabled);
    }

    #[test]
    fn backward_compat_old_toml_without_device_serial() {
        let toml_str = r##"
mute_color = "#FF0000"
hotkey = "Ctrl+Shift+M"
sound_enabled = true
autostart = false
mute_inputs = "all"
"##;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert!(c.device_serial.is_empty());
    }

    // ── input_colors validation ──

    #[test]
    fn validate_input_colors_valid() {
        let c = Config {
            input_colors: HashMap::from([
                ("1".into(), "#FF0000".into()),
                ("2".into(), "blue".into()),
            ]),
            ..Config::default()
        };
        assert!(c.validate(Some(2), 10_000_000).is_ok());
    }

    #[test]
    fn validate_input_colors_invalid_color_value() {
        let c = Config {
            input_colors: HashMap::from([("1".into(), "not-a-color".into())]),
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::InvalidInputColor { input, .. } if input == "1"
        )));
    }

    #[test]
    fn validate_input_colors_out_of_range_key() {
        let c = Config {
            input_colors: HashMap::from([("5".into(), "#FF0000".into())]),
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::InvalidInputColor { reason, .. } if reason.contains("out of range")
        )));
    }

    #[test]
    fn validate_input_colors_non_numeric_key() {
        let c = Config {
            input_colors: HashMap::from([("abc".into(), "#FF0000".into())]),
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::InvalidInputColor { reason, .. } if reason.contains("1-based input number")
        )));
    }

    #[test]
    fn validate_input_colors_key_range_skipped_without_input_count() {
        let c = Config {
            input_colors: HashMap::from([("99".into(), "#FF0000".into())]),
            ..Config::default()
        };
        // Without input_count, key range check is skipped (only color value is validated)
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn load_ignores_header_comment() {
        // Config with header comment (as produced by save()) should parse fine
        let toml_str = r##"# FocusMute configuration — changes made outside the app may be overwritten.

mute_color = "#00FF00"
hotkey = "F12"
sound_enabled = false
autostart = true
mute_inputs = "1,2"
mute_sound_path = ""
unmute_sound_path = ""
device_serial = ""
on_mute_command = ""
on_unmute_command = ""
notifications_enabled = false
"##;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.mute_color, "#00FF00");
        assert_eq!(c.hotkey, "F12");
        assert!(!c.sound_enabled);
        assert!(c.autostart);
        assert_eq!(c.mute_inputs, "1,2");
    }

    // ── save_to / load_from ──

    #[test]
    fn save_to_load_from_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = Config {
            mute_color: "#00FF00".into(),
            hotkey: "Alt+M".into(),
            sound_enabled: false,
            autostart: true,
            mute_inputs: "1,2".into(),
            mute_sound_path: "/tmp/mute.wav".into(),
            unmute_sound_path: "/tmp/unmute.wav".into(),
            device_serial: "ABC123".into(),
            on_mute_command: "echo muted".into(),
            on_unmute_command: "echo unmuted".into(),
            input_colors: HashMap::from([
                ("1".into(), "#FF0000".into()),
                ("2".into(), "#0000FF".into()),
            ]),
            notifications_enabled: true,
        };
        config.save_to(&path).unwrap();

        let (loaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(loaded.mute_color, config.mute_color);
        assert_eq!(loaded.hotkey, config.hotkey);
        assert_eq!(loaded.sound_enabled, config.sound_enabled);
        assert_eq!(loaded.autostart, config.autostart);
        assert_eq!(loaded.mute_inputs, config.mute_inputs);
        assert_eq!(loaded.mute_sound_path, config.mute_sound_path);
        assert_eq!(loaded.unmute_sound_path, config.unmute_sound_path);
        assert_eq!(loaded.device_serial, config.device_serial);
        assert_eq!(loaded.on_mute_command, config.on_mute_command);
        assert_eq!(loaded.on_unmute_command, config.on_unmute_command);
        assert_eq!(loaded.input_colors, config.input_colors);
        assert_eq!(loaded.notifications_enabled, config.notifications_enabled);
    }

    #[test]
    fn save_to_includes_header_comment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        Config::default().save_to(&path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.starts_with("# FocusMute configuration"),
            "saved file should start with header comment"
        );
    }

    #[test]
    fn save_to_cleans_up_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        Config::default().save_to(&path).unwrap();
        let tmp = dir.path().join("config.toml.tmp");
        assert!(!tmp.exists(), "temp file should not remain after save");
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");

        let (config, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(config.mute_color, "#FF0000");
        assert_eq!(config.hotkey, "Ctrl+Shift+M");
    }

    #[test]
    fn load_from_invalid_toml_returns_defaults_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is { not valid toml").unwrap();

        let (config, warnings) = Config::load_from(&path);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("config parse error"));
        assert_eq!(config.mute_color, "#FF0000");
    }
}
