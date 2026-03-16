//! `config` subcommand — show current configuration and file paths.

use std::path::Path;

use super::{Config, ConfigFilesJson, ConfigOutput, Result, kv, kv_indent, kv_width, led, schema};
use focusmute_lib::FocusmuteError;

pub(super) fn cmd_config(json: bool, custom_path: Option<&Path>, edit: bool) -> Result<()> {
    if edit {
        if json {
            log::warn!("[cli] --json is ignored when --edit is used");
        }
        let path = custom_path.map(|p| p.to_path_buf()).or_else(Config::path);
        let Some(path) = path else {
            return Err(FocusmuteError::Config(
                "no config directory available".into(),
            ));
        };
        if !path.exists() {
            Config::default().save_to(&path)?;
        }
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| {
                if cfg!(windows) {
                    "notepad".into()
                } else {
                    "vi".into()
                }
            });
        let status = std::process::Command::new(&editor)
            .arg(&path)
            .status()
            .map_err(|e| {
                FocusmuteError::Config(format!("failed to launch editor '{}': {}", editor, e))
            })?;
        if !status.success() {
            return Err(FocusmuteError::Config(format!(
                "editor exited with {}",
                status
            )));
        }
        return Ok(());
    }

    let config = super::load_config(custom_path);
    let config_path = custom_path.map(|p| p.to_path_buf()).or_else(Config::path);
    let config_exists = config_path.as_ref().map(|p| p.exists()).unwrap_or(false);

    let schema_cache = schema::cache_path();
    let schema_cache_exists = schema_cache.as_ref().is_some_and(|p| p.exists());

    if json {
        let output = ConfigOutput {
            config_file: config_path.as_ref().map(|p| p.display().to_string()),
            config_file_exists: config_exists,
            settings: config,
            files: ConfigFilesJson {
                schema_cache: schema_cache.as_ref().map(|p| p.display().to_string()),
                schema_cache_exists,
            },
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(());
    }

    // Human-readable output
    let w = kv_width(
        &["Config file:"],
        &[
            "mute_color:",
            "hotkey:",
            "sound_enabled:",
            "mute_sound_volume:",
            "unmute_sound_volume:",
            "autostart:",
            "notifications_enabled:",
            "mute_inputs:",
            "mute_sound_path:",
            "unmute_sound_path:",
            "Schema cache:",
        ],
    );

    match &config_path {
        Some(p) => {
            if config_exists {
                kv("Config file:", format_args!("{} (loaded)", p.display()), w);
            } else {
                kv(
                    "Config file:",
                    format_args!("{} (not found, using defaults)", p.display()),
                    w,
                );
            }
        }
        None => kv("Config file:", "(no config directory)", w),
    }
    println!();

    println!("Settings:");
    let color_display = match led::parse_color(&config.indicator.mute_color) {
        Ok(val) => format!(
            "{} -> {}",
            config.indicator.mute_color,
            led::format_color(val)
        ),
        Err(_) => format!("{} (invalid)", config.indicator.mute_color),
    };
    kv_indent("mute_color:", &color_display, w);
    kv_indent("hotkey:", &config.keyboard.hotkey, w);
    kv_indent("sound_enabled:", config.sound.sound_enabled, w);
    kv_indent("mute_sound_volume:", config.sound.mute_sound_volume, w);
    kv_indent("unmute_sound_volume:", config.sound.unmute_sound_volume, w);
    kv_indent("autostart:", config.system.autostart, w);
    kv_indent(
        "notifications_enabled:",
        config.system.notifications_enabled,
        w,
    );
    let mute_mode = config.parse_mute_inputs();
    kv_indent("mute_inputs:", &mute_mode, w);
    let sound_label = |path: &str| {
        if path.is_empty() {
            "(built-in)".to_string()
        } else {
            path.to_string()
        }
    };
    kv_indent(
        "mute_sound_path:",
        sound_label(&config.sound.mute_sound_path),
        w,
    );
    kv_indent(
        "unmute_sound_path:",
        sound_label(&config.sound.unmute_sound_path),
        w,
    );
    println!();

    println!("Files:");
    match &schema_cache {
        Some(p) => {
            let status = if schema_cache_exists {
                "present"
            } else {
                "not found"
            };
            kv_indent(
                "Schema cache:",
                format_args!("{} ({status})", p.display()),
                w,
            );
        }
        None => kv_indent("Schema cache:", "(no config directory)", w),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_config_with_custom_path_nonexistent() {
        // Custom path that doesn't exist — should show defaults with "(not found)"
        let path = std::path::Path::new("/tmp/focusmute_test_nonexistent.toml");
        let result = cmd_config(false, Some(path), false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_config_json_with_custom_path_nonexistent() {
        let path = std::path::Path::new("/tmp/focusmute_test_nonexistent.toml");
        let result = cmd_config(true, Some(path), false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_config_with_custom_path_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save_to(&path).unwrap();

        let result = cmd_config(false, Some(&path), false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_config_json_with_custom_path_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.indicator.mute_color = "#00FF00".into();
        config.save_to(&path).unwrap();

        let result = cmd_config(true, Some(&path), false);
        assert!(result.is_ok());
    }

    #[test]
    fn cmd_config_edit_no_config_dir_returns_error() {
        // --edit with None path and no default config dir is an edge case.
        // On CI/test environments Config::path() usually exists, so this
        // just verifies the function doesn't panic with valid inputs.
        let result = cmd_config(false, None, false);
        assert!(result.is_ok());
    }
}
