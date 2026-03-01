//! Cross-platform egui settings dialog.

use std::sync::{Arc, Mutex};

use eframe::egui;
use focusmute_lib::config::Config;
use focusmute_lib::led;

use super::{MAX_SOUND_FILE_BYTES, SoundPreviewPlayer, combo_to_mute_inputs, inputs_combo_items};

/// Tracks which side of the color sync last changed.
#[derive(PartialEq)]
pub(crate) enum ColorDirty {
    Neither,
    Text,
    Picker,
}

pub struct SettingsApp {
    // ── Form state ──
    color_text: String,
    color_rgb: [f32; 3],
    color_dirty: ColorDirty,

    hotkey: String,

    mute_inputs_index: usize,
    mute_inputs_items: Vec<String>,
    input_count: usize,

    sound_enabled: bool,
    autostart: bool,

    mute_sound_path: String,
    unmute_sound_path: String,

    on_mute_command: String,
    on_unmute_command: String,

    // ── Sound preview ──
    preview_player: Option<SoundPreviewPlayer>,

    // ── Non-editable fields carried through ──
    original: Config,

    // ── About section (read-only) ──
    device_lines: Vec<(String, String)>,

    // ── Validation ──
    errors: Vec<String>,

    // ── Shared result (read by caller after run_native returns) ──
    result: Arc<Mutex<Option<Config>>>,

    /// Resize the viewport on the next frame.
    needs_resize: bool,
    /// Previous collapsible section openness — resize while animating.
    prev_advanced_openness: f32,
    prev_about_openness: f32,
}

impl SettingsApp {
    pub fn new(
        config: Config,
        input_count: usize,
        device_lines: Vec<(String, String)>,
        result: Arc<Mutex<Option<Config>>>,
        cc: &eframe::CreationContext<'_>,
    ) -> Self {
        // Apply widget style customizations
        let mut style = (*cc.egui_ctx.style()).clone();
        let corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.noninteractive.corner_radius = corner_radius;
        style.visuals.widgets.inactive.corner_radius = corner_radius;
        style.visuals.widgets.active.corner_radius = corner_radius;
        style.visuals.widgets.hovered.corner_radius = corner_radius;
        cc.egui_ctx.set_style(style);

        let color_rgb = hex_to_rgb(&config.mute_color).unwrap_or([1.0, 0.0, 0.0]);
        let (mute_inputs_items, mute_inputs_index) = inputs_combo_items(&config, input_count);

        Self {
            color_text: config.mute_color.clone(),
            color_rgb,
            color_dirty: ColorDirty::Neither,

            hotkey: config.hotkey.clone(),

            mute_inputs_index,
            mute_inputs_items,
            input_count,

            sound_enabled: config.sound_enabled,
            autostart: config.autostart,

            mute_sound_path: config.mute_sound_path.clone(),
            unmute_sound_path: config.unmute_sound_path.clone(),

            on_mute_command: config.on_mute_command.clone(),
            on_unmute_command: config.on_unmute_command.clone(),

            preview_player: None,

            original: config,

            device_lines,

            errors: Vec::new(),

            result,

            needs_resize: true,
            prev_advanced_openness: -1.0,
            prev_about_openness: -1.0,
        }
    }

    /// Try to save: validate, send result, and close on success.
    fn try_save(&mut self, ctx: &egui::Context) {
        match build_and_validate_config(&ValidateParams {
            color_dirty: &self.color_dirty,
            color_text: &self.color_text,
            color_rgb: self.color_rgb,
            hotkey: &self.hotkey,
            sound_enabled: self.sound_enabled,
            autostart: self.autostart,
            mute_inputs_index: self.mute_inputs_index,
            input_count: self.input_count,
            mute_sound_path: &self.mute_sound_path,
            unmute_sound_path: &self.unmute_sound_path,
            on_mute_command: &self.on_mute_command,
            on_unmute_command: &self.on_unmute_command,
            original: &self.original,
            max_sound_bytes: MAX_SOUND_FILE_BYTES,
        }) {
            Ok(config) => {
                *self.result.lock().unwrap() = Some(config);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(errs) => {
                self.errors = errs;
            }
        }
    }

    fn cancel(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Height of the button area below content (separator + padding + buttons).
        const BUTTON_AREA_HEIGHT: f32 = 54.0;

        let mut content_bottom = 0.0_f32;
        let mut advanced_openness = 0.0_f32;
        let mut about_openness = 0.0_f32;
        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Mute Indicator section ──
            section_frame(ui, "Mute Indicator", |ui| {
                egui::Grid::new("mute_indicator_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        // Mute Inputs row
                        ui.label("Mute Inputs");
                        let selected_text = self
                            .mute_inputs_items
                            .get(self.mute_inputs_index)
                            .cloned()
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("mute_inputs_combo")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (i, item) in self.mute_inputs_items.iter().enumerate() {
                                    ui.selectable_value(&mut self.mute_inputs_index, i, item);
                                }
                            });
                        ui.end_row();

                        // Color row
                        ui.label("Mute Color");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let before = self.color_rgb;
                            ui.color_edit_button_rgb(&mut self.color_rgb);
                            if self.color_rgb != before {
                                self.color_dirty = ColorDirty::Picker;
                                self.color_text = rgb_to_hex(self.color_rgb);
                            }

                            let text_response = ui.add(
                                egui::TextEdit::singleline(&mut self.color_text)
                                    .desired_width(ui.available_width()),
                            );
                            if text_response.changed() {
                                self.color_dirty = ColorDirty::Text;
                                if let Some(rgb) = hex_to_rgb(&self.color_text) {
                                    self.color_rgb = rgb;
                                }
                            }
                        });
                        ui.end_row();
                    });
            });

            // ── Hotkey section ──
            section_frame(ui, "Hotkey", |ui| {
                let text_width = (ui.available_width() - 80.0 - 12.0).max(100.0);
                egui::Grid::new("hotkey_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Hotkey");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hotkey).desired_width(text_width),
                        );
                        ui.end_row();
                    });
            });

            // ── Sound section ──
            section_frame(ui, "Sound", |ui| {
                ui.checkbox(&mut self.sound_enabled, "Sound Feedback");
                ui.add_space(4.0);

                egui::Grid::new("sound_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Mute Sound");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Play").clicked() {
                                if self.preview_player.is_none() {
                                    self.preview_player = SoundPreviewPlayer::try_new();
                                }
                                if let Some(ref player) = self.preview_player {
                                    player.play(&self.mute_sound_path, crate::sound::SOUND_MUTED);
                                }
                            }
                            if !self.mute_sound_path.is_empty() && ui.button("Clear").clicked() {
                                self.mute_sound_path.clear();
                            }
                            if ui.button("Browse...").clicked()
                                && let Some(path) = browse_wav_file()
                            {
                                self.mute_sound_path = path;
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.mute_sound_path)
                                    .desired_width(ui.available_width())
                                    .hint_text("(built-in)"),
                            );
                        });
                        ui.end_row();

                        ui.label("Unmute Sound");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Play").clicked() {
                                if self.preview_player.is_none() {
                                    self.preview_player = SoundPreviewPlayer::try_new();
                                }
                                if let Some(ref player) = self.preview_player {
                                    player
                                        .play(&self.unmute_sound_path, crate::sound::SOUND_UNMUTED);
                                }
                            }
                            if !self.unmute_sound_path.is_empty() && ui.button("Clear").clicked() {
                                self.unmute_sound_path.clear();
                            }
                            if ui.button("Browse...").clicked()
                                && let Some(path) = browse_wav_file()
                            {
                                self.unmute_sound_path = path;
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.unmute_sound_path)
                                    .desired_width(ui.available_width())
                                    .hint_text("(built-in)"),
                            );
                        });
                        ui.end_row();
                    });
            });

            // ── System section ──
            section_frame(ui, "System", |ui| {
                #[cfg(windows)]
                ui.checkbox(&mut self.autostart, "Start with Windows");
                #[cfg(not(windows))]
                ui.checkbox(&mut self.autostart, "Start with System");
            });

            // ── Advanced section (collapsible, collapsed by default) ──
            ui.add_space(6.0);
            let advanced_header =
                egui::CollapsingHeader::new(egui::RichText::new("Advanced").strong().size(14.0))
                    .default_open(false)
                    .show_unindented(ui, |ui| {
                        egui::Frame::group(ui.style())
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let text_width = ui.available_width() - 4.0;
                                ui.label("On Mute Command");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.on_mute_command)
                                        .desired_width(text_width)
                                        .hint_text("(none)"),
                                );
                                ui.add_space(4.0);
                                ui.label("On Unmute Command");
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.on_unmute_command)
                                        .desired_width(text_width)
                                        .hint_text("(none)"),
                                );
                            });
                    });
            advanced_openness = advanced_header.openness;

            // ── About section (collapsible, collapsed by default) ──
            ui.add_space(6.0);
            let about_header =
                egui::CollapsingHeader::new(egui::RichText::new("About").strong().size(14.0))
                    .default_open(false)
                    .show_unindented(ui, |ui| {
                        egui::Frame::group(ui.style())
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let version = env!("CARGO_PKG_VERSION");
                                ui.label(
                                    egui::RichText::new(format!("FocusMute v{version}"))
                                        .strong()
                                        .size(15.0),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    "Hotkey mute control for Focusrite Scarlett 4th Gen interfaces",
                                );
                                ui.add_space(6.0);

                                egui::Grid::new("about_device_grid")
                                    .num_columns(2)
                                    .spacing([8.0, 4.0])
                                    .show(ui, |ui| {
                                        for (key, val) in &self.device_lines {
                                            ui.label(
                                                egui::RichText::new(format!("{key}:")).strong(),
                                            );
                                            ui.label(val);
                                            ui.end_row();
                                        }
                                        ui.label("");
                                        ui.end_row();
                                        ui.label(egui::RichText::new("Source:").strong());
                                        ui.hyperlink_to(
                                            "github.com/barnumbirr/focusmute",
                                            "https://github.com/barnumbirr/focusmute",
                                        );
                                        ui.end_row();
                                    });
                            });
                    });
            about_openness = about_header.openness;

            // ── Errors area ──
            if !self.errors.is_empty() {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                for err in &self.errors {
                    ui.label(egui::RichText::new(err).color(egui::Color32::from_rgb(220, 50, 50)));
                }
            }

            // Measure content height BEFORE the button layout. The right-to-left
            // layout below consumes all remaining vertical space, so measuring
            // after it would return the window height (causing a feedback loop).
            content_bottom = ui.cursor().top();

            // ── Buttons ──
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0); // right padding
                let save_btn = egui::Button::new("Save")
                    .fill(egui::Color32::from_rgb(60, 130, 210))
                    .min_size(egui::vec2(80.0, 0.0));
                if ui.add(save_btn).clicked() {
                    self.try_save(ui.ctx());
                }

                let cancel_btn = egui::Button::new("Cancel")
                    .fill(egui::Color32::from_rgb(80, 80, 85))
                    .min_size(egui::vec2(80.0, 0.0));
                if ui.add(cancel_btn).clicked() {
                    self.cancel(ui.ctx());
                }
            });
        });

        // Resize on the first frame and while any collapsible section animates.
        // content_bottom is measured before the right-to-left button layout,
        // so it reflects actual content height and doesn't depend on window
        // size — no feedback loop.
        let advanced_animating = (advanced_openness - self.prev_advanced_openness).abs() > 0.001;
        let about_animating = (about_openness - self.prev_about_openness).abs() > 0.001;
        self.prev_advanced_openness = advanced_openness;
        self.prev_about_openness = about_openness;

        if self.needs_resize || advanced_animating || about_animating {
            self.needs_resize = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                440.0,
                (content_bottom + BUTTON_AREA_HEIGHT).round(),
            )));
        }
    }
}

/// Parameters for [`build_and_validate_config`], grouping dialog form fields.
pub(crate) struct ValidateParams<'a> {
    pub color_dirty: &'a ColorDirty,
    pub color_text: &'a str,
    pub color_rgb: [f32; 3],
    pub hotkey: &'a str,
    pub sound_enabled: bool,
    pub autostart: bool,
    pub mute_inputs_index: usize,
    pub input_count: usize,
    pub mute_sound_path: &'a str,
    pub unmute_sound_path: &'a str,
    pub on_mute_command: &'a str,
    pub on_unmute_command: &'a str,
    pub original: &'a Config,
    pub max_sound_bytes: u64,
}

/// Build a `Config` from dialog form fields, validate, and return it or a list of error strings.
///
/// This is a pure function (no UI side effects) to enable unit testing.
pub(crate) fn build_and_validate_config(p: &ValidateParams<'_>) -> Result<Config, Vec<String>> {
    let mute_inputs = combo_to_mute_inputs(p.mute_inputs_index, p.input_count);

    // Sync color from picker if that was the last change
    let color_str = if *p.color_dirty == ColorDirty::Picker {
        rgb_to_hex(p.color_rgb)
    } else {
        p.color_text.to_string()
    };

    let candidate = Config {
        mute_color: color_str,
        hotkey: p.hotkey.to_string(),
        sound_enabled: p.sound_enabled,
        autostart: p.autostart,
        mute_inputs,
        mute_sound_path: p.mute_sound_path.to_string(),
        unmute_sound_path: p.unmute_sound_path.to_string(),
        device_serial: p.original.device_serial.clone(),
        on_mute_command: p.on_mute_command.to_string(),
        on_unmute_command: p.on_unmute_command.to_string(),
        input_colors: p.original.input_colors.clone(),
        notifications_enabled: p.original.notifications_enabled,
    };

    let input_count_opt = if p.input_count > 0 {
        Some(p.input_count)
    } else {
        None
    };

    let mut errors = Vec::new();

    if let Err(errs) = candidate.validate(input_count_opt, p.max_sound_bytes) {
        for e in &errs {
            errors.push(e.to_string());
        }
    }

    // Validate hotkey syntax (global-hotkey crate parsing)
    let hotkey_str = p.hotkey.trim();
    if !hotkey_str.is_empty() && hotkey_str.parse::<global_hotkey::hotkey::HotKey>().is_err() {
        errors.push(format!("Invalid hotkey syntax: \"{hotkey_str}\""));
    }

    if errors.is_empty() {
        Ok(candidate)
    } else {
        Err(errors)
    }
}

/// Render a section with a title and grouped frame that spans the full width.
fn section_frame(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong().size(14.0));
    ui.add_space(2.0);
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            // Fix both min and max to the frame's available width so all
            // sections render at the same width.
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

// ── Color helpers ──

/// Parse a color string (hex or named) into normalized RGB floats [0.0, 1.0].
pub fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let device_val = led::parse_color(hex).ok()?;
    let r = ((device_val >> 24) & 0xFF) as f32 / 255.0;
    let g = ((device_val >> 16) & 0xFF) as f32 / 255.0;
    let b = ((device_val >> 8) & 0xFF) as f32 / 255.0;
    Some([r, g, b])
}

/// Convert normalized RGB floats to a `#RRGGBB` hex string.
pub fn rgb_to_hex(rgb: [f32; 3]) -> String {
    let r = (rgb[0] * 255.0).round() as u8;
    let g = (rgb[1] * 255.0).round() as u8;
    let b = (rgb[2] * 255.0).round() as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Show a native file dialog filtered to WAV files.
fn browse_wav_file() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("WAV", &["wav"])
        .pick_file()
        .and_then(|p| p.to_str().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Helper: build a valid config with defaults, returning Ok.
    fn valid_build() -> Result<Config, Vec<String>> {
        build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        })
    }

    #[test]
    fn build_valid_inputs_returns_ok() {
        let config = valid_build().expect("should be Ok");
        assert_eq!(config.mute_color, "#FF0000");
        assert_eq!(config.hotkey, "Ctrl+Shift+M");
        assert!(config.sound_enabled);
        assert!(!config.autostart);
        assert_eq!(config.mute_inputs, "all");
    }

    #[test]
    fn build_invalid_color_returns_err() {
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Text,
            color_text: "not-a-color",
            color_rgb: [0.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_lowercase().contains("color")),
            "expected color error, got: {errs:?}"
        );
    }

    #[test]
    fn build_empty_hotkey_returns_err() {
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        });
        // Empty hotkey triggers the Config::validate error (hotkey required)
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_lowercase().contains("hotkey")),
            "expected hotkey error, got: {errs:?}"
        );
    }

    #[test]
    fn build_invalid_hotkey_syntax_returns_err() {
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Blah",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("Invalid hotkey syntax")),
            "expected hotkey syntax error, got: {errs:?}"
        );
    }

    #[test]
    fn build_picker_dirty_uses_rgb_conversion() {
        let config = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Picker,
            color_text: "garbage-text",
            color_rgb: [0.0, 1.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        })
        .expect("picker dirty should use RGB, not text");
        assert_eq!(config.mute_color, "#00FF00");
    }

    #[test]
    fn build_preserves_original_fields() {
        let original = Config {
            device_serial: "ABC123".to_string(),
            input_colors: HashMap::from([("1".into(), "#00FF00".into())]),
            notifications_enabled: true,
            ..Config::default()
        };

        let config = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original: &original,
            max_sound_bytes: 10_000_000,
        })
        .expect("should be Ok");

        assert_eq!(config.device_serial, "ABC123");
        assert_eq!(config.input_colors.get("1").unwrap(), "#00FF00");
        assert!(config.notifications_enabled);
    }

    #[test]
    fn build_hooks_are_preserved() {
        let config = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            autostart: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "echo muted",
            on_unmute_command: "echo unmuted",
            original: &Config::default(),
            max_sound_bytes: 10_000_000,
        })
        .expect("should be Ok");

        assert_eq!(config.on_mute_command, "echo muted");
        assert_eq!(config.on_unmute_command, "echo unmuted");
    }

    #[test]
    fn hex_to_rgb_valid_hex() {
        let rgb = hex_to_rgb("#FF0000").unwrap();
        assert!((rgb[0] - 1.0).abs() < 0.01);
        assert!(rgb[1].abs() < 0.01);
        assert!(rgb[2].abs() < 0.01);
    }

    #[test]
    fn hex_to_rgb_named_color() {
        let rgb = hex_to_rgb("blue").unwrap();
        assert!(rgb[0].abs() < 0.01);
        assert!(rgb[1].abs() < 0.01);
        assert!((rgb[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn hex_to_rgb_invalid() {
        assert!(hex_to_rgb("chartreuse").is_none());
        assert!(hex_to_rgb("#GGG").is_none());
    }

    #[test]
    fn rgb_to_hex_roundtrip() {
        let rgb = [1.0, 0.0, 0.0];
        assert_eq!(rgb_to_hex(rgb), "#FF0000");
    }

    #[test]
    fn rgb_to_hex_mixed() {
        let rgb = [0.0, 0.5, 1.0];
        let hex = rgb_to_hex(rgb);
        assert_eq!(hex, "#0080FF");
    }

    #[test]
    fn hex_rgb_roundtrip() {
        for color in &[
            "#FF0000", "#00FF00", "#0000FF", "#ABCDEF", "#000000", "#FFFFFF",
        ] {
            let rgb = hex_to_rgb(color).unwrap();
            let back = rgb_to_hex(rgb);
            assert_eq!(&back, color, "roundtrip failed for {color}");
        }
    }

    #[test]
    fn hex_rgb_roundtrip_named() {
        // Named colors roundtrip through their hex representation
        let rgb = hex_to_rgb("red").unwrap();
        assert_eq!(rgb_to_hex(rgb), "#FF0000");

        let rgb = hex_to_rgb("green").unwrap();
        assert_eq!(rgb_to_hex(rgb), "#00FF00");
    }
}
