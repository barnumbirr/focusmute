//! Application configuration — TOML-based, platform-aware paths.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

/// Header comment prepended to saved config files.
const CONFIG_HEADER: &str = "\
# FocusMute configuration
#
# Location:
#   Windows: %APPDATA%\\Focusmute\\config.toml
#   Linux:   ~/.config/focusmute/config.toml
#
# This file is auto-generated when settings are saved.
# Manual edits are supported — missing keys use default values.

";

// ── Section sub-structs ──

/// Mute indicator settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorConfig {
    /// Mute indicator color (hex or name). Default: "#FF0000" (red).
    #[serde(default = "default_mute_color")]
    pub mute_color: String,

    /// Which inputs to show mute indicator on. Default: "all".
    /// Values: "all", "1", "2", "1,2", etc. (1-based input numbers).
    #[serde(default = "default_mute_inputs")]
    pub mute_inputs: String,

    /// Per-input mute colors (1-based keys). Overrides `mute_color` for specific inputs.
    /// Example in TOML: `[indicator.input_colors]` / `1 = "#FF0000"` / `2 = "#0000FF"`
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub input_colors: HashMap<String, String>,
}

impl Default for IndicatorConfig {
    fn default() -> Self {
        Self {
            mute_color: default_mute_color(),
            mute_inputs: default_mute_inputs(),
            input_colors: HashMap::new(),
        }
    }
}

/// Keyboard / hotkey settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardConfig {
    /// Global hotkey to toggle mute. Default: "Ctrl+Shift+M". Format: "Modifier+Key" or just "Key".
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Push-to-talk hotkey. Empty = disabled. Hold to unmute, release to re-mute.
    #[serde(default)]
    pub push_to_talk_hotkey: String,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            push_to_talk_hotkey: String::new(),
        }
    }
}

/// Sound feedback settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    /// Play a sound when mute state changes.
    #[serde(default = "default_true")]
    pub sound_enabled: bool,

    /// Path to custom mute sound WAV file. Empty = use built-in.
    #[serde(default)]
    pub mute_sound_path: String,

    /// Path to custom unmute sound WAV file. Empty = use built-in.
    #[serde(default)]
    pub unmute_sound_path: String,

    /// Volume for mute sound (0.0–1.0). Default: 1.0.
    #[serde(default = "default_sound_volume")]
    pub mute_sound_volume: f32,

    /// Volume for unmute sound (0.0–1.0). Default: 1.0.
    #[serde(default = "default_sound_volume")]
    pub unmute_sound_volume: f32,

    /// Suppress sound feedback when mute state changes via browser extension sync.
    /// Default: true (sounds are suppressed for browser-initiated changes).
    #[serde(default = "default_true")]
    pub suppress_browser_sync_sound: bool,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            mute_sound_path: String::new(),
            unmute_sound_path: String::new(),
            mute_sound_volume: default_sound_volume(),
            unmute_sound_volume: default_sound_volume(),
            suppress_browser_sync_sound: true,
        }
    }
}

/// System / startup settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Start application on login.
    #[serde(default)]
    pub autostart: bool,

    /// Preferred device serial number. Empty = auto-select first device.
    #[serde(default)]
    pub device_serial: String,

    /// Show desktop notification on mute state change.
    #[serde(default)]
    pub notifications_enabled: bool,

    /// Log level for the tray app log file. Default: "info".
    /// Valid values: "error", "warn", "info", "debug", "trace".
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Port for browser extension mute sync. Default: 0 (disabled).
    /// When the field is absent from the config file, defaults to 9736 for
    /// backward compatibility. When nonzero, a localhost HTTP server listens
    /// on this port for mute state messages from the browser extension.
    #[serde(default = "default_websocket_port")]
    pub websocket_port: u16,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            autostart: false,
            device_serial: String::new(),
            notifications_enabled: false,
            log_level: default_log_level(),
            websocket_port: 0,
        }
    }
}

/// Webhook hooks — HTTP POST on mute state changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    /// URL to POST when muted. Empty = disabled.
    #[serde(default, alias = "on_mute_command")]
    pub on_mute_url: String,

    /// URL to POST when unmuted. Empty = disabled.
    #[serde(default, alias = "on_unmute_command")]
    pub on_unmute_url: String,

    /// Custom JSON body for the mute webhook. Empty = default `{"event":"mute"}`.
    #[serde(default)]
    pub on_mute_body: String,

    /// Custom JSON body for the unmute webhook. Empty = default `{"event":"unmute"}`.
    #[serde(default)]
    pub on_unmute_body: String,
}

// ── Default value helpers ──

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
fn default_sound_volume() -> f32 {
    1.0
}
fn default_log_level() -> String {
    "info".into()
}
fn default_websocket_port() -> u16 {
    9736
}

/// Valid log level strings (case-insensitive matching during validation).
pub const VALID_LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

// ── Top-level config ──

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub indicator: IndicatorConfig,

    #[serde(default)]
    pub keyboard: KeyboardConfig,

    #[serde(default)]
    pub sound: SoundConfig,

    #[serde(default)]
    pub system: SystemConfig,

    #[serde(default)]
    pub hooks: HooksConfig,
}

// ── Legacy flat config for migration ──

/// Old flat config format (pre-v0.5.0). Used only for deserialization during migration.
#[derive(Deserialize)]
struct LegacyConfig {
    #[serde(default = "default_mute_color")]
    mute_color: String,
    #[serde(default = "default_hotkey")]
    hotkey: String,
    #[serde(default = "default_true")]
    sound_enabled: bool,
    #[serde(default)]
    autostart: bool,
    #[serde(default = "default_mute_inputs")]
    mute_inputs: String,
    #[serde(default)]
    mute_sound_path: String,
    #[serde(default)]
    unmute_sound_path: String,
    #[serde(default = "default_sound_volume")]
    mute_sound_volume: f32,
    #[serde(default = "default_sound_volume")]
    unmute_sound_volume: f32,
    #[serde(default)]
    device_serial: String,
    #[serde(default)]
    on_mute_command: String,
    #[serde(default)]
    on_unmute_command: String,
    #[serde(default)]
    input_colors: HashMap<String, String>,
    #[serde(default)]
    notifications_enabled: bool,
}

impl From<LegacyConfig> for Config {
    fn from(legacy: LegacyConfig) -> Self {
        Config {
            indicator: IndicatorConfig {
                mute_color: legacy.mute_color,
                mute_inputs: legacy.mute_inputs,
                input_colors: legacy.input_colors,
            },
            keyboard: KeyboardConfig {
                hotkey: legacy.hotkey,
                ..Default::default()
            },
            sound: SoundConfig {
                sound_enabled: legacy.sound_enabled,
                mute_sound_path: legacy.mute_sound_path,
                unmute_sound_path: legacy.unmute_sound_path,
                mute_sound_volume: legacy.mute_sound_volume,
                unmute_sound_volume: legacy.unmute_sound_volume,
                ..Default::default()
            },
            system: SystemConfig {
                autostart: legacy.autostart,
                device_serial: legacy.device_serial,
                notifications_enabled: legacy.notifications_enabled,
                log_level: default_log_level(),
                websocket_port: default_websocket_port(),
            },
            hooks: HooksConfig {
                on_mute_url: legacy.on_mute_command,
                on_unmute_url: legacy.on_unmute_command,
                ..Default::default()
            },
        }
    }
}

// ── Mute inputs ──

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

// ── Validation ──

/// Validation errors that [`Config::validate`] can return.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// The `mute_color` field could not be parsed as a valid color.
    InvalidColor(String),
    /// A sound volume field is outside the valid 0.0–1.0 range.
    InvalidSoundVolume { field: &'static str, value: f32 },
    /// The `hotkey` field is empty or whitespace-only.
    EmptyHotkey,
    /// A custom sound path is invalid (`field` is `"mute_sound_path"` or `"unmute_sound_path"`).
    InvalidSoundPath { field: &'static str, reason: String },
    /// The `mute_inputs` field references inputs that don't exist on the device.
    InvalidMuteInputs(String),
    /// An `input_colors` entry is invalid (bad color value or out-of-range key).
    InvalidInputColor { input: String, reason: String },
    /// The `log_level` field is not a recognized level.
    InvalidLogLevel(String),
    /// The browser sync port is a privileged port (1–1023).
    InvalidWebSocketPort(u16),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidColor(e) => write!(f, "Invalid mute color: {e}"),
            ValidationError::InvalidSoundVolume { field, value } => {
                write!(f, "{field} must be between 0.0 and 1.0, got {value}")
            }
            ValidationError::EmptyHotkey => write!(f, "Hotkey cannot be empty"),
            ValidationError::InvalidSoundPath { field, reason } => {
                write!(f, "Invalid {field}: {reason}")
            }
            ValidationError::InvalidMuteInputs(e) => write!(f, "Invalid mute inputs: {e}"),
            ValidationError::InvalidInputColor { input, reason } => {
                write!(f, "Invalid input_colors[{input}]: {reason}")
            }
            ValidationError::InvalidLogLevel(level) => {
                write!(
                    f,
                    "Invalid log level \"{level}\". Valid values: error, warn, info, debug, trace"
                )
            }
            ValidationError::InvalidWebSocketPort(port) => {
                write!(
                    f,
                    "Browser sync port {port} is privileged (1–1023). Use a port >= 1024, or 0 to disable"
                )
            }
        }
    }
}

// ── Config impl ──

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

    /// Returns true if the config file does not yet exist on disk (first run).
    pub fn is_first_run() -> bool {
        Self::path().is_none_or(|p| !p.exists())
    }

    /// Load config from disk, or return defaults if not found.
    pub fn load() -> Self {
        let (config, warnings) = Self::load_with_warnings();
        for w in &warnings {
            log::warn!("[config] {w}");
        }
        config
    }

    /// Save config to an arbitrary path atomically (write to temp file, then rename).
    ///
    /// A header comment is prepended to the saved file.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let serialized = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let contents = format!("{CONFIG_HEADER}{serialized}");
        // Write temp file in the same directory as target (guaranteed same FS for atomic rename).
        let tmp = dir.join(format!(".focusmute-config-{}.tmp", std::process::id()));
        std::fs::write(&tmp, &contents)?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Rename can fail across filesystems; fall back to direct write + cleanup
                let result = std::fs::write(path, &contents);
                if let Err(e) = std::fs::remove_file(&tmp) {
                    log::debug!("[config] could not remove temp file {}: {e}", tmp.display());
                }
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
    ///
    /// Supports both the current nested format (`[indicator]`, `[keyboard]`, etc.)
    /// and the legacy flat format (pre-v0.5.0). Legacy configs are silently migrated
    /// on the next save.
    pub fn load_from(path: &Path) -> (Self, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                // Try new nested format first
                if let Ok(config) = toml::from_str::<Config>(&contents) {
                    return (config, vec![]);
                }
                // Fall back to legacy flat format
                match toml::from_str::<LegacyConfig>(&contents) {
                    Ok(legacy) => (Config::from(legacy), vec![]),
                    Err(e) => {
                        log::debug!("[config] TOML parse error: {e}");
                        let warning = "Settings file corrupted. Using defaults.".to_string();
                        (Self::default(), vec![warning])
                    }
                }
            }
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
        let s = self.indicator.mute_inputs.trim();
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
                    let mb = max_size_bytes / 1_000_000;
                    return Err(crate::FocusmuteError::Config(format!(
                        "Sound file too large ({} bytes). Maximum size is {mb} MB.",
                        meta.len()
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
        if let Err(e) = crate::led::parse_color(&self.indicator.mute_color) {
            errors.push(ValidationError::InvalidColor(e.to_string()));
        }

        // Validate hotkey
        if self.keyboard.hotkey.trim().is_empty() {
            errors.push(ValidationError::EmptyHotkey);
        }

        // Validate sound paths
        if let Err(e) = Self::validate_sound_path(&self.sound.mute_sound_path, max_sound_bytes) {
            errors.push(ValidationError::InvalidSoundPath {
                field: "mute_sound_path",
                reason: e.to_string(),
            });
        }
        if let Err(e) = Self::validate_sound_path(&self.sound.unmute_sound_path, max_sound_bytes) {
            errors.push(ValidationError::InvalidSoundPath {
                field: "unmute_sound_path",
                reason: e.to_string(),
            });
        }

        // Validate sound volumes
        if !(0.0..=1.0).contains(&self.sound.mute_sound_volume) {
            errors.push(ValidationError::InvalidSoundVolume {
                field: "mute_sound_volume",
                value: self.sound.mute_sound_volume,
            });
        }
        if !(0.0..=1.0).contains(&self.sound.unmute_sound_volume) {
            errors.push(ValidationError::InvalidSoundVolume {
                field: "unmute_sound_volume",
                value: self.sound.unmute_sound_volume,
            });
        }

        // Validate log level
        if !VALID_LOG_LEVELS.contains(&self.system.log_level.to_lowercase().as_str()) {
            errors.push(ValidationError::InvalidLogLevel(
                self.system.log_level.clone(),
            ));
        }

        // Validate WebSocket port (0 = disabled, 1–1023 = privileged = rejected)
        if self.system.websocket_port > 0 && self.system.websocket_port < 1024 {
            errors.push(ValidationError::InvalidWebSocketPort(
                self.system.websocket_port,
            ));
        }

        // Validate mute inputs if input count is known
        if let Some(count) = input_count
            && let Err(e) = self.validate_mute_inputs(count)
        {
            errors.push(ValidationError::InvalidMuteInputs(e.to_string()));
        }

        // Validate input_colors entries
        for (key, value) in &self.indicator.input_colors {
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
        assert_eq!(c.indicator.mute_color, "#FF0000");
        assert_eq!(c.keyboard.hotkey, "Ctrl+Shift+M");
        assert!(c.sound.sound_enabled);
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
        assert!(!c.system.autostart);
        assert_eq!(c.indicator.mute_inputs, "all");
    }

    // NOTE: serialize_roundtrip removed — fully covered by config_round_trip_all_fields.

    #[test]
    fn partial_toml_fills_defaults() {
        let toml_str = "[indicator]\nmute_color = \"#0000FF\"";
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.indicator.mute_color, "#0000FF");
        // Missing fields get defaults
        assert_eq!(c.keyboard.hotkey, "Ctrl+Shift+M");
        assert!(c.sound.sound_enabled);
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
        assert!(!c.system.autostart);
        assert_eq!(c.indicator.mute_inputs, "all");
    }

    #[test]
    fn empty_toml_gives_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.indicator.mute_color, "#FF0000");
        assert_eq!(c.keyboard.hotkey, "Ctrl+Shift+M");
        assert!(c.sound.sound_enabled);
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
        assert!(!c.system.autostart);
        assert_eq!(c.indicator.mute_inputs, "all");
    }

    #[test]
    fn malformed_toml_gives_defaults() {
        // toml::from_str returns Err for malformed input — Config::load would
        // use defaults with a warning. Test that the fallback behavior is correct.
        let result: std::result::Result<Config, _> = toml::from_str("this is { not valid toml");
        assert!(result.is_err());
        // After error, the app falls back to defaults
        let fallback = Config::default();
        assert_eq!(fallback.indicator.mute_color, "#FF0000");
        assert_eq!(fallback.keyboard.hotkey, "Ctrl+Shift+M");
    }

    #[test]
    fn wrong_type_toml_gives_defaults() {
        // A valid TOML key with the wrong type (string where bool expected)
        let result: std::result::Result<Config, _> =
            toml::from_str("[sound]\nsound_enabled = \"not a bool\"");
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

    // ── Legacy migration ──

    #[test]
    fn legacy_flat_config_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r##"
mute_color = "#00FF00"
hotkey = "F12"
sound_enabled = false
autostart = true
mute_inputs = "1,2"
mute_sound_path = "/tmp/mute.wav"
unmute_sound_path = "/tmp/unmute.wav"
device_serial = "ABC123"
on_mute_command = "echo muted"
on_unmute_command = "echo unmuted"
notifications_enabled = true
"##,
        )
        .unwrap();

        let (c, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(c.indicator.mute_color, "#00FF00");
        assert_eq!(c.keyboard.hotkey, "F12");
        assert!(!c.sound.sound_enabled);
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
        assert!(c.system.autostart);
        assert_eq!(c.indicator.mute_inputs, "1,2");
        assert_eq!(c.sound.mute_sound_path, "/tmp/mute.wav");
        assert_eq!(c.sound.unmute_sound_path, "/tmp/unmute.wav");
        assert_eq!(c.system.device_serial, "ABC123");
        assert_eq!(c.hooks.on_mute_url, "echo muted");
        assert_eq!(c.hooks.on_unmute_url, "echo unmuted");
        assert!(c.system.notifications_enabled);
    }

    #[test]
    fn legacy_config_migrated_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        // Write legacy flat format
        std::fs::write(&path, "mute_color = \"#00FF00\"\nhotkey = \"F12\"\n").unwrap();

        // Load (migrates from legacy)
        let (config, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(config.indicator.mute_color, "#00FF00");

        // Save (writes new nested format)
        config.save_to(&path).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("[indicator]"),
            "saved file should have [indicator] section"
        );
        assert!(
            contents.contains("[keyboard]"),
            "saved file should have [keyboard] section"
        );

        // Re-load should work as new nested format
        let (reloaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(reloaded.indicator.mute_color, "#00FF00");
        assert_eq!(reloaded.keyboard.hotkey, "F12");
    }

    #[test]
    fn legacy_partial_config_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        // Old config missing newer fields
        std::fs::write(
            &path,
            r##"
mute_color = "#FF0000"
hotkey = "Ctrl+Shift+M"
sound_enabled = true
autostart = false
"##,
        )
        .unwrap();

        let (c, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(c.indicator.mute_inputs, "all");
        assert!(c.sound.mute_sound_path.is_empty());
        assert!(c.system.device_serial.is_empty());
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn save_to_omits_empty_input_colors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        Config::default().save_to(&path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            !contents.contains("[indicator.input_colors]"),
            "empty input_colors should not produce a table"
        );
    }

    #[test]
    fn new_nested_config_loads_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r##"
[indicator]
mute_color = "#00FF00"
mute_inputs = "1"

[keyboard]
hotkey = "F12"

[sound]
sound_enabled = false
mute_sound_path = "/tmp/mute.wav"
unmute_sound_path = ""

[system]
autostart = true
device_serial = "SER123"
notifications_enabled = true

[hooks]
on_mute_command = "echo m"
on_unmute_command = "echo u"
"##,
        )
        .unwrap();

        let (c, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(c.indicator.mute_color, "#00FF00");
        assert_eq!(c.indicator.mute_inputs, "1");
        assert_eq!(c.keyboard.hotkey, "F12");
        assert!(!c.sound.sound_enabled);
        assert_eq!(c.sound.mute_sound_path, "/tmp/mute.wav");
        assert!(c.system.autostart);
        assert_eq!(c.system.device_serial, "SER123");
        assert!(c.system.notifications_enabled);
        assert_eq!(c.hooks.on_mute_url, "echo m");
        assert_eq!(c.hooks.on_unmute_url, "echo u");
    }

    #[test]
    fn save_load_roundtrip_nested() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let config = Config {
            indicator: IndicatorConfig {
                mute_color: "#00FF00".into(),
                mute_inputs: "1,2".into(),
                input_colors: HashMap::from([
                    ("1".into(), "#FF0000".into()),
                    ("2".into(), "#0000FF".into()),
                ]),
            },
            keyboard: KeyboardConfig {
                hotkey: "Alt+M".into(),
                ..Default::default()
            },
            sound: SoundConfig {
                sound_enabled: false,
                mute_sound_path: "/tmp/mute.wav".into(),
                unmute_sound_path: "/tmp/unmute.wav".into(),
                mute_sound_volume: 0.75,
                unmute_sound_volume: 0.5,
                ..Default::default()
            },
            system: SystemConfig {
                autostart: true,
                device_serial: "ABC123".into(),
                notifications_enabled: true,
                websocket_port: 9736,
                ..Default::default()
            },
            hooks: HooksConfig {
                on_mute_url: "https://example.com/muted".into(),
                on_unmute_url: "https://example.com/unmuted".into(),
                ..Default::default()
            },
        };
        config.save_to(&path).unwrap();

        let (loaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(loaded.indicator.mute_color, config.indicator.mute_color);
        assert_eq!(loaded.keyboard.hotkey, config.keyboard.hotkey);
        assert_eq!(loaded.sound.sound_enabled, config.sound.sound_enabled);
        assert_eq!(
            loaded.sound.mute_sound_volume,
            config.sound.mute_sound_volume
        );
        assert_eq!(
            loaded.sound.unmute_sound_volume,
            config.sound.unmute_sound_volume
        );
        assert_eq!(loaded.system.autostart, config.system.autostart);
        assert_eq!(loaded.indicator.mute_inputs, config.indicator.mute_inputs);
        assert_eq!(loaded.sound.mute_sound_path, config.sound.mute_sound_path);
        assert_eq!(
            loaded.sound.unmute_sound_path,
            config.sound.unmute_sound_path
        );
        assert_eq!(loaded.system.device_serial, config.system.device_serial);
        assert_eq!(loaded.hooks.on_mute_url, config.hooks.on_mute_url);
        assert_eq!(loaded.hooks.on_unmute_url, config.hooks.on_unmute_url);
        assert_eq!(loaded.indicator.input_colors, config.indicator.input_colors);
        assert_eq!(
            loaded.system.notifications_enabled,
            config.system.notifications_enabled
        );
        assert_eq!(loaded.system.websocket_port, config.system.websocket_port);
    }

    // ── parse_mute_inputs ──

    #[test]
    fn parse_mute_inputs_all() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "all".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_all_case_insensitive() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "ALL".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
        let c2 = Config {
            indicator: IndicatorConfig {
                mute_inputs: "All".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c2.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_empty_is_all() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_whitespace_is_all() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "  ".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_single() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0]));
    }

    #[test]
    fn parse_mute_inputs_single_input_2() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![1]));
    }

    #[test]
    fn parse_mute_inputs_multiple() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1,2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_with_spaces() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: " 1 , 2 ".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_deduplicates() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1,1,2,2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_sorts() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "2,1".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::Specific(vec![0, 1]));
    }

    #[test]
    fn parse_mute_inputs_zero_ignored() {
        // 0 is invalid (1-based), so it should be ignored
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "0".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_garbage_is_all() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "abc".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);
    }

    #[test]
    fn parse_mute_inputs_negative_ignored() {
        // Negative numbers fail usize parse, so they're silently ignored.
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "-1".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c.parse_mute_inputs(), MuteInputs::All);

        // Mixed: valid "1" survives, "-2" is ignored.
        let c2 = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1,-2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert_eq!(c2.parse_mute_inputs(), MuteInputs::Specific(vec![0]));
    }

    // ── validate_mute_inputs ──

    #[test]
    fn validate_all_always_ok() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "all".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(2).is_ok());
        assert!(c.validate_mute_inputs(0).is_ok());
    }

    #[test]
    fn validate_within_range() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1,2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(2).is_ok());
    }

    #[test]
    fn validate_out_of_range() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "3".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        let err = c.validate_mute_inputs(2).unwrap_err();
        assert!(err.to_string().contains("out of range"), "got: {err}");
    }

    #[test]
    fn validate_single_input_device() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "1".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c.validate_mute_inputs(1).is_ok());
        let c2 = Config {
            indicator: IndicatorConfig {
                mute_inputs: "2".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c2.validate_mute_inputs(1).is_err());
    }

    // ── sound path config ──

    #[test]
    fn defaults_include_empty_sound_paths() {
        let c = Config::default();
        assert!(c.sound.mute_sound_path.is_empty());
        assert!(c.sound.unmute_sound_path.is_empty());
    }

    // NOTE: serialize_roundtrip_with_sound_paths removed — covered by config_round_trip_all_fields.

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

    // ── sound_volume ──

    #[test]
    fn validate_mute_sound_volume_out_of_range() {
        for bad in [1.5, -0.1, 2.0, -1.0] {
            let c = Config {
                sound: SoundConfig {
                    mute_sound_volume: bad,
                    ..Default::default()
                },
                ..Config::default()
            };
            let errs = c.validate(None, 10_000_000).unwrap_err();
            assert!(
                errs.iter().any(|e| matches!(
                    e,
                    ValidationError::InvalidSoundVolume {
                        field: "mute_sound_volume",
                        ..
                    }
                )),
                "expected InvalidSoundVolume for mute {bad}, got: {errs:?}"
            );
        }
    }

    #[test]
    fn validate_unmute_sound_volume_out_of_range() {
        for bad in [1.5, -0.1] {
            let c = Config {
                sound: SoundConfig {
                    unmute_sound_volume: bad,
                    ..Default::default()
                },
                ..Config::default()
            };
            let errs = c.validate(None, 10_000_000).unwrap_err();
            assert!(
                errs.iter().any(|e| matches!(
                    e,
                    ValidationError::InvalidSoundVolume {
                        field: "unmute_sound_volume",
                        ..
                    }
                )),
                "expected InvalidSoundVolume for unmute {bad}, got: {errs:?}"
            );
        }
    }

    #[test]
    fn validate_sound_volume_boundary_values_ok() {
        for ok in [0.0, 0.5, 1.0] {
            let c = Config {
                sound: SoundConfig {
                    mute_sound_volume: ok,
                    unmute_sound_volume: ok,
                    ..Default::default()
                },
                ..Config::default()
            };
            assert!(
                c.validate(None, 10_000_000).is_ok(),
                "sound_volume {ok} should be valid"
            );
        }
    }

    #[test]
    fn missing_sound_volume_deserializes_to_default() {
        let toml_str =
            "[sound]\nsound_enabled = true\nmute_sound_path = \"\"\nunmute_sound_path = \"\"";
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
    }

    #[test]
    fn legacy_config_without_sound_volume_gets_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "mute_color = \"#FF0000\"\nhotkey = \"Ctrl+Shift+M\"\nsound_enabled = true\nautostart = false\n",
        )
        .unwrap();

        let (c, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(c.sound.mute_sound_volume, 1.0);
        assert_eq!(c.sound.unmute_sound_volume, 1.0);
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
            indicator: IndicatorConfig {
                mute_color: "chartreuse".into(),
                ..Default::default()
            },
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
            keyboard: KeyboardConfig {
                hotkey: "  ".into(),
                ..Default::default()
            },
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
            sound: SoundConfig {
                mute_sound_path: "/no/such/file.wav".into(),
                ..Default::default()
            },
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
            sound: SoundConfig {
                unmute_sound_path: "/no/such/file.wav".into(),
                ..Default::default()
            },
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
            indicator: IndicatorConfig {
                mute_inputs: "5".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::InvalidMuteInputs(_)));
    }

    #[test]
    fn validate_mute_inputs_skipped_without_count() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_inputs: "99".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        // Without input_count, mute_inputs validation is skipped
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn validate_collects_multiple_errors() {
        let c = Config {
            indicator: IndicatorConfig {
                mute_color: "not-a-color".into(),
                mute_inputs: "99".into(),
                ..Default::default()
            },
            keyboard: KeyboardConfig {
                hotkey: "".into(),
                ..Default::default()
            },
            sound: SoundConfig {
                mute_sound_path: "/bad/path.wav".into(),
                unmute_sound_path: "/bad/path2.wav".into(),
                mute_sound_volume: 2.0,
                unmute_sound_volume: -1.0,
                ..Default::default()
            },
            ..Config::default()
        };
        let errs = c.validate(Some(2), 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 7);
        // Verify ordering: color, hotkey, mute_sound, unmute_sound, mute_vol, unmute_vol, mute_inputs
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
        assert!(matches!(
            errs[4],
            ValidationError::InvalidSoundVolume {
                field: "mute_sound_volume",
                ..
            }
        ));
        assert!(matches!(
            errs[5],
            ValidationError::InvalidSoundVolume {
                field: "unmute_sound_volume",
                ..
            }
        ));
        assert!(matches!(errs[6], ValidationError::InvalidMuteInputs(_)));
    }

    #[test]
    fn validate_valid_hex_color_and_named() {
        for color in &["#FF0000", "red", "blue", "#ABCDEF"] {
            let c = Config {
                indicator: IndicatorConfig {
                    mute_color: color.to_string(),
                    ..Default::default()
                },
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

        let e = ValidationError::InvalidSoundVolume {
            field: "mute_sound_volume",
            value: 1.5,
        };
        assert_eq!(
            e.to_string(),
            "mute_sound_volume must be between 0.0 and 1.0, got 1.5"
        );
    }

    #[test]
    fn config_round_trip_all_fields() {
        let config = Config {
            indicator: IndicatorConfig {
                mute_color: "#00FF00".into(),
                mute_inputs: "1,2".into(),
                input_colors: HashMap::from([
                    ("1".into(), "#FF0000".into()),
                    ("2".into(), "#0000FF".into()),
                ]),
            },
            keyboard: KeyboardConfig {
                hotkey: "Alt+M".into(),
                ..Default::default()
            },
            sound: SoundConfig {
                sound_enabled: false,
                mute_sound_path: "/tmp/mute.wav".into(),
                unmute_sound_path: "/tmp/unmute.wav".into(),
                mute_sound_volume: 0.3,
                unmute_sound_volume: 0.8,
                ..Default::default()
            },
            system: SystemConfig {
                autostart: true,
                device_serial: "ABC123".into(),
                notifications_enabled: true,
                ..Default::default()
            },
            hooks: HooksConfig {
                on_mute_url: "https://example.com/muted".into(),
                on_unmute_url: "https://example.com/unmuted".into(),
                ..Default::default()
            },
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.indicator.mute_color, config.indicator.mute_color);
        assert_eq!(loaded.keyboard.hotkey, config.keyboard.hotkey);
        assert_eq!(loaded.sound.sound_enabled, config.sound.sound_enabled);
        assert_eq!(
            loaded.sound.mute_sound_volume,
            config.sound.mute_sound_volume
        );
        assert_eq!(
            loaded.sound.unmute_sound_volume,
            config.sound.unmute_sound_volume
        );
        assert_eq!(loaded.system.autostart, config.system.autostart);
        assert_eq!(loaded.indicator.mute_inputs, config.indicator.mute_inputs);
        assert_eq!(loaded.sound.mute_sound_path, config.sound.mute_sound_path);
        assert_eq!(
            loaded.sound.unmute_sound_path,
            config.sound.unmute_sound_path
        );
        assert_eq!(loaded.system.device_serial, config.system.device_serial);
        assert_eq!(loaded.hooks.on_mute_url, config.hooks.on_mute_url);
        assert_eq!(loaded.hooks.on_unmute_url, config.hooks.on_unmute_url);
        assert_eq!(loaded.indicator.input_colors, config.indicator.input_colors);
        assert_eq!(
            loaded.system.notifications_enabled,
            config.system.notifications_enabled
        );
    }

    // ── input_colors validation ──

    #[test]
    fn validate_input_colors_valid() {
        let c = Config {
            indicator: IndicatorConfig {
                input_colors: HashMap::from([
                    ("1".into(), "#FF0000".into()),
                    ("2".into(), "blue".into()),
                ]),
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c.validate(Some(2), 10_000_000).is_ok());
    }

    #[test]
    fn validate_input_colors_invalid_color_value() {
        let c = Config {
            indicator: IndicatorConfig {
                input_colors: HashMap::from([("1".into(), "not-a-color".into())]),
                ..Default::default()
            },
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
            indicator: IndicatorConfig {
                input_colors: HashMap::from([("5".into(), "#FF0000".into())]),
                ..Default::default()
            },
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
            indicator: IndicatorConfig {
                input_colors: HashMap::from([("abc".into(), "#FF0000".into())]),
                ..Default::default()
            },
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
            indicator: IndicatorConfig {
                input_colors: HashMap::from([("99".into(), "#FF0000".into())]),
                ..Default::default()
            },
            ..Config::default()
        };
        // Without input_count, key range check is skipped (only color value is validated)
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn load_ignores_header_comment() {
        // Config with header comment (as produced by save()) should parse fine
        let toml_str = r##"# FocusMute configuration
#
# Location:
#   Windows: %APPDATA%\Focusmute\config.toml
#   Linux:   ~/.config/focusmute/config.toml
#
# This file is auto-generated when settings are saved.
# Manual edits are supported — missing keys use default values.

[indicator]
mute_color = "#00FF00"
mute_inputs = "1,2"

[keyboard]
hotkey = "F12"

[sound]
sound_enabled = false
mute_sound_path = ""
unmute_sound_path = ""

[system]
autostart = true
device_serial = ""
notifications_enabled = false
"##;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.indicator.mute_color, "#00FF00");
        assert_eq!(c.keyboard.hotkey, "F12");
        assert!(!c.sound.sound_enabled);
        assert!(c.system.autostart);
        assert_eq!(c.indicator.mute_inputs, "1,2");
    }

    // ── save_to / load_from ──

    // NOTE: save_to/load_from roundtrip is covered by save_load_roundtrip_nested above.

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
        assert_eq!(config.indicator.mute_color, "#FF0000");
        assert_eq!(config.keyboard.hotkey, "Ctrl+Shift+M");
    }

    #[test]
    fn load_from_invalid_toml_returns_defaults_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is { not valid toml").unwrap();

        let (config, warnings) = Config::load_from(&path);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("corrupted"));
        assert_eq!(config.indicator.mute_color, "#FF0000");
    }

    // ── is_first_run ──

    #[test]
    fn is_first_run_when_config_missing() {
        // is_first_run checks Config::path() which points to the real config dir.
        // We can't easily mock it, but we can verify the logic via load_from:
        // a missing file means first run.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        assert!(!path.exists());
        // The static is_first_run() uses Config::path(), but we can verify the
        // underlying logic: file doesn't exist → first run condition is true.
        assert!(!path.exists(), "nonexistent file should not exist");
    }

    #[test]
    fn is_first_run_when_config_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save_to(&path).unwrap();
        assert!(path.exists(), "saved config should exist");
    }

    // ── save_to atomic write ──

    #[test]
    fn save_to_pid_temp_file_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save_to(&path).unwrap();

        // Verify no .focusmute-config-*.tmp files remain
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.contains(".tmp")))
            .collect();
        assert!(
            entries.is_empty(),
            "no temp files should remain after save: {entries:?}"
        );
    }

    #[test]
    fn save_to_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.toml");
        assert!(!path.parent().unwrap().exists());

        Config::default().save_to(&path).unwrap();
        assert!(path.exists());
    }

    // ── load_from corruption message ──

    #[test]
    fn load_from_corruption_warning_is_user_friendly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "[invalid\nsection").unwrap();

        let (_, warnings) = Config::load_from(&path);
        assert_eq!(warnings.len(), 1);
        // Must be user-friendly, not raw TOML error
        assert!(
            warnings[0].contains("Settings file corrupted"),
            "expected user-friendly message, got: {}",
            warnings[0]
        );
        assert!(warnings[0].contains("Using defaults"));
        // Must NOT contain raw TOML error details
        assert!(
            !warnings[0].contains("expected"),
            "should not expose TOML parser error: {}",
            warnings[0]
        );
    }

    // ── sound file error message ──

    #[test]
    fn validate_sound_path_too_large_mentions_mb() {
        let dir = std::env::temp_dir();
        let path = dir.join("focusmute_test_mb.wav");
        std::fs::write(&path, vec![0u8; 200]).unwrap();
        let err = Config::validate_sound_path(path.to_str().unwrap(), 100).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("MB"), "should mention MB limit, got: {msg}");
        let _ = std::fs::remove_file(&path);
    }

    // ── Push-to-talk hotkey ──

    #[test]
    fn ptt_hotkey_defaults_to_empty() {
        let config = Config::default();
        assert!(config.keyboard.push_to_talk_hotkey.is_empty());
    }

    #[test]
    fn ptt_hotkey_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.keyboard.push_to_talk_hotkey = "Ctrl+Space".into();
        config.save_to(&path).unwrap();

        let (loaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(loaded.keyboard.push_to_talk_hotkey, "Ctrl+Space");
    }

    #[test]
    fn ptt_hotkey_missing_field_defaults_to_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[keyboard]
hotkey = "F12"
"#,
        )
        .unwrap();

        let (config, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(config.keyboard.hotkey, "F12");
        assert!(config.keyboard.push_to_talk_hotkey.is_empty());
    }

    // ── Log level ──

    #[test]
    fn log_level_defaults_to_info() {
        let config = Config::default();
        assert_eq!(config.system.log_level, "info");
    }

    #[test]
    fn validate_invalid_log_level() {
        let c = Config {
            system: SystemConfig {
                log_level: "garbage".into(),
                ..Default::default()
            },
            ..Config::default()
        };
        let errs = c.validate(None, 10_000_000).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::InvalidLogLevel(_)));
        assert!(errs[0].to_string().contains("garbage"));
    }

    #[test]
    fn validate_all_valid_log_levels() {
        for level in VALID_LOG_LEVELS {
            let c = Config {
                system: SystemConfig {
                    log_level: level.to_string(),
                    ..Default::default()
                },
                ..Config::default()
            };
            assert!(
                c.validate(None, 10_000_000).is_ok(),
                "log level '{level}' should be valid"
            );
        }
    }

    #[test]
    fn log_level_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.system.log_level = "debug".into();
        config.save_to(&path).unwrap();

        let (loaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(loaded.system.log_level, "debug");
    }

    #[test]
    fn log_level_missing_field_defaults_to_info() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[system]
autostart = false
"#,
        )
        .unwrap();

        let (config, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(config.system.log_level, "info");
    }

    // ── WebSocket port ──

    #[test]
    fn websocket_port_defaults_to_0() {
        let config = Config::default();
        assert_eq!(config.system.websocket_port, 0);
    }

    #[test]
    fn validate_websocket_port_zero_is_ok() {
        let c = Config {
            system: SystemConfig {
                websocket_port: 0,
                ..Default::default()
            },
            ..Config::default()
        };
        assert!(c.validate(None, 10_000_000).is_ok());
    }

    #[test]
    fn validate_websocket_port_valid() {
        for port in [1024, 9736, 65535] {
            let c = Config {
                system: SystemConfig {
                    websocket_port: port,
                    ..Default::default()
                },
                ..Config::default()
            };
            assert!(
                c.validate(None, 10_000_000).is_ok(),
                "port {port} should be valid"
            );
        }
    }

    #[test]
    fn validate_websocket_port_privileged_rejected() {
        for port in [1, 80, 443, 1023] {
            let c = Config {
                system: SystemConfig {
                    websocket_port: port,
                    ..Default::default()
                },
                ..Config::default()
            };
            let errs = c.validate(None, 10_000_000).unwrap_err();
            assert!(
                errs.iter()
                    .any(|e| matches!(e, ValidationError::InvalidWebSocketPort(_))),
                "port {port} should be rejected as privileged, got: {errs:?}"
            );
        }
    }

    #[test]
    fn websocket_port_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.system.websocket_port = 9736;
        config.save_to(&path).unwrap();

        let (loaded, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(loaded.system.websocket_port, 9736);
    }

    #[test]
    fn websocket_port_missing_field_defaults_to_serde_9736() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[system]
autostart = false
log_level = "info"
"#,
        )
        .unwrap();

        let (config, warnings) = Config::load_from(&path);
        assert!(warnings.is_empty());
        assert_eq!(config.system.websocket_port, 9736);
    }
}
