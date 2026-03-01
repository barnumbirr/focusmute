//! `config` subcommand — show current configuration and file paths.

use std::path::Path;

use super::{Config, ConfigFilesJson, ConfigOutput, Result, kv, kv_indent, kv_width, led, schema};

pub(super) fn cmd_config(json: bool, custom_path: Option<&Path>) -> Result<()> {
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
            "autostart:",
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
    let color_display = match led::parse_color(&config.mute_color) {
        Ok(val) => format!("{} -> {}", config.mute_color, led::format_color(val)),
        Err(_) => format!("{} (invalid)", config.mute_color),
    };
    kv_indent("mute_color:", &color_display, w);
    kv_indent("hotkey:", &config.hotkey, w);
    kv_indent("sound_enabled:", config.sound_enabled, w);
    kv_indent("autostart:", config.autostart, w);
    let mute_mode = config.parse_mute_inputs();
    kv_indent("mute_inputs:", &mute_mode, w);
    let sound_label = |path: &str| {
        if path.is_empty() {
            "(built-in)".to_string()
        } else {
            path.to_string()
        }
    };
    kv_indent("mute_sound_path:", sound_label(&config.mute_sound_path), w);
    kv_indent(
        "unmute_sound_path:",
        sound_label(&config.unmute_sound_path),
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
