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
    mute_sound_volume: f32,
    unmute_sound_volume: f32,
    autostart: bool,
    notifications_enabled: bool,

    mute_sound_path: String,
    unmute_sound_path: String,

    on_mute_command: String,
    on_unmute_command: String,

    // ── Sound preview ──
    preview_player: SoundPreviewPlayer,

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

        let color_rgb = led::parse_color(&config.indicator.mute_color)
            .ok()
            .map(led::color_to_rgb)
            .unwrap_or([1.0, 0.0, 0.0]);
        let (mute_inputs_items, mute_inputs_index) = inputs_combo_items(&config, input_count);

        Self {
            color_text: config.indicator.mute_color.clone(),
            color_rgb,
            color_dirty: ColorDirty::Neither,

            hotkey: config.keyboard.hotkey.clone(),

            mute_inputs_index,
            mute_inputs_items,
            input_count,

            sound_enabled: config.sound.sound_enabled,
            mute_sound_volume: config.sound.mute_sound_volume,
            unmute_sound_volume: config.sound.unmute_sound_volume,
            autostart: config.system.autostart,
            notifications_enabled: config.system.notifications_enabled,

            mute_sound_path: config.sound.mute_sound_path.clone(),
            unmute_sound_path: config.sound.unmute_sound_path.clone(),

            on_mute_command: config.hooks.on_mute_command.clone(),
            on_unmute_command: config.hooks.on_unmute_command.clone(),

            preview_player: SoundPreviewPlayer::new(),

            original: config,

            device_lines,

            errors: Vec::new(),

            result,

            needs_resize: true,
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
            mute_sound_volume: self.mute_sound_volume,
            unmute_sound_volume: self.unmute_sound_volume,
            autostart: self.autostart,
            notifications_enabled: self.notifications_enabled,
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

    /// Snapshot all form fields for change detection (used to clear stale errors).
    fn form_snapshot(&self) -> FormSnapshot {
        FormSnapshot {
            color_text: self.color_text.clone(),
            color_rgb: self.color_rgb,
            hotkey: self.hotkey.clone(),
            mute_inputs_index: self.mute_inputs_index,
            sound_enabled: self.sound_enabled,
            mute_sound_volume: self.mute_sound_volume,
            unmute_sound_volume: self.unmute_sound_volume,
            autostart: self.autostart,
            notifications_enabled: self.notifications_enabled,
            mute_sound_path: self.mute_sound_path.clone(),
            unmute_sound_path: self.unmute_sound_path.clone(),
            on_mute_command: self.on_mute_command.clone(),
            on_unmute_command: self.on_unmute_command.clone(),
        }
    }
}

/// All form fields snapshotted for change detection — no element-count limit.
#[derive(PartialEq)]
struct FormSnapshot {
    color_text: String,
    color_rgb: [f32; 3],
    hotkey: String,
    mute_inputs_index: usize,
    sound_enabled: bool,
    mute_sound_volume: f32,
    unmute_sound_volume: f32,
    autostart: bool,
    notifications_enabled: bool,
    mute_sound_path: String,
    unmute_sound_path: String,
    on_mute_command: String,
    on_unmute_command: String,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Height of the button area below content (separator + padding + buttons).
        const BUTTON_AREA_HEIGHT: f32 = 54.0;

        // Snapshot form state before rendering — if anything changes,
        // clear stale validation errors so the Save button stays reachable.
        let form_snap = self.form_snapshot();

        let mut content_bottom = 0.0_f32;
        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Mute Indicator section ──
            section_frame(ui, "Mute Indicator", |ui| {
                egui::Grid::new("mute_indicator_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        // Mute Inputs row
                        ui.label("Mute Inputs")
                            .on_hover_text("Which input LEDs show the mute color");
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
                                self.color_text = led::rgb_to_hex(self.color_rgb);
                            }

                            let text_response = ui.add(
                                egui::TextEdit::singleline(&mut self.color_text)
                                    .desired_width(ui.available_width())
                                    .hint_text("#FF0000 or red"),
                            );
                            if text_response.changed() {
                                self.color_dirty = ColorDirty::Text;
                                if let Ok(val) = led::parse_color(&self.color_text) {
                                    self.color_rgb = led::color_to_rgb(val);
                                }
                            }
                        });
                        ui.end_row();
                    });
            });

            // ── Keyboard section ──
            section_frame(ui, "Keyboard", |ui| {
                let text_width = (ui.available_width() - 80.0 - 12.0).max(100.0);
                egui::Grid::new("hotkey_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Hotkey");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hotkey)
                                .desired_width(text_width)
                                .hint_text("e.g. Ctrl+Shift+M"),
                        );
                        ui.end_row();
                    });
            });

            // ── Sound section ──
            section_frame(ui, "Sound", |ui| {
                ui.checkbox(&mut self.sound_enabled, "Sound Feedback");
                ui.add_space(4.0);

                // Measure "Browse..." text width so Play buttons can match.
                let browse_text_width = ui.fonts(|f| {
                    f.layout_no_wrap(
                        "Browse...".into(),
                        egui::TextStyle::Button.resolve(ui.style()),
                        egui::Color32::WHITE,
                    )
                    .size()
                    .x
                });
                let browse_btn_width = (browse_text_width + ui.spacing().button_padding.x * 2.0)
                    .max(ui.spacing().interact_size.x);

                egui::Grid::new("sound_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Mute Sound");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

                        volume_row(
                            ui,
                            browse_btn_width,
                            &mut self.mute_sound_volume,
                            &self.mute_sound_path,
                            crate::sound::SOUND_MUTED,
                            &mut self.preview_player,
                        );

                        ui.label("Unmute Sound");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

                        volume_row(
                            ui,
                            browse_btn_width,
                            &mut self.unmute_sound_volume,
                            &self.unmute_sound_path,
                            crate::sound::SOUND_UNMUTED,
                            &mut self.preview_player,
                        );
                    });
            });

            // ── System section ──
            section_frame(ui, "System", |ui| {
                #[cfg(windows)]
                ui.checkbox(&mut self.autostart, "Start with Windows");
                #[cfg(not(windows))]
                ui.checkbox(&mut self.autostart, "Start with System");
                ui.checkbox(&mut self.notifications_enabled, "Desktop notifications");
            });

            // ── Advanced section (collapsible, collapsed by default) ──
            ui.add_space(6.0);
            egui::CollapsingHeader::new(egui::RichText::new("Advanced").strong().size(14.0))
                .default_open(false)
                .show_unindented(ui, |ui| {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let text_width = ui.available_width() - 4.0;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Hooks").strong());
                                ui.label("ℹ").on_hover_ui(|ui| {
                                    ui.label("Shell commands run when mute state changes.");
                                    ui.horizontal(|ui| {
                                        ui.label("Example:");
                                        ui.label(
                                            egui::RichText::new(
                                                "curl -X POST https://example.com/webhook",
                                            )
                                            .monospace(),
                                        );
                                    });
                                });
                            });
                            ui.add_space(2.0);
                            ui.label("On mute");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_mute_command)
                                    .desired_width(text_width)
                                    .hint_text("(none)"),
                            );
                            ui.add_space(4.0);
                            ui.label("On unmute");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_unmute_command)
                                    .desired_width(text_width)
                                    .hint_text("(none)"),
                            );
                        });
                });
            // ── About section (collapsible, collapsed by default) ──
            ui.add_space(6.0);
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
                                        ui.label(egui::RichText::new(format!("{key}:")).strong());
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

        // Clear validation errors when any form field changes.
        if !self.errors.is_empty() && form_snap != self.form_snapshot() {
            self.errors.clear();
        }

        // Always enforce content-driven height (locks vertical resize) while
        // preserving the user's chosen width (horizontal resize is free).
        let target_height = (content_bottom + BUTTON_AREA_HEIGHT).round();
        let current_width = ctx
            .input(|i| i.viewport().inner_rect)
            .map(|r| r.width())
            .unwrap_or(440.0);
        let width = if self.needs_resize {
            440.0
        } else {
            current_width
        };
        self.needs_resize = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            width,
            target_height,
        )));
    }
}

/// Parameters for [`build_and_validate_config`], grouping dialog form fields.
pub(crate) struct ValidateParams<'a> {
    pub color_dirty: &'a ColorDirty,
    pub color_text: &'a str,
    pub color_rgb: [f32; 3],
    pub hotkey: &'a str,
    pub sound_enabled: bool,
    pub mute_sound_volume: f32,
    pub unmute_sound_volume: f32,
    pub autostart: bool,
    pub notifications_enabled: bool,
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
        led::rgb_to_hex(p.color_rgb)
    } else {
        p.color_text.to_string()
    };

    let candidate = Config {
        indicator: focusmute_lib::config::IndicatorConfig {
            mute_color: color_str,
            mute_inputs,
            input_colors: p.original.indicator.input_colors.clone(),
        },
        keyboard: focusmute_lib::config::KeyboardConfig {
            hotkey: p.hotkey.to_string(),
        },
        sound: focusmute_lib::config::SoundConfig {
            sound_enabled: p.sound_enabled,
            mute_sound_path: p.mute_sound_path.to_string(),
            unmute_sound_path: p.unmute_sound_path.to_string(),
            mute_sound_volume: p.mute_sound_volume,
            unmute_sound_volume: p.unmute_sound_volume,
        },
        system: focusmute_lib::config::SystemConfig {
            autostart: p.autostart,
            device_serial: p.original.system.device_serial.clone(),
            notifications_enabled: p.notifications_enabled,
        },
        hooks: focusmute_lib::config::HooksConfig {
            on_mute_command: p.on_mute_command.to_string(),
            on_unmute_command: p.on_unmute_command.to_string(),
        },
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
        errors.push("Invalid hotkey. Examples: Ctrl+Shift+M, Alt+F1".to_string());
    }

    if errors.is_empty() {
        Ok(candidate)
    } else {
        Err(errors)
    }
}

/// Render a volume slider row inside a sound grid (label + RTL: slider, DragValue, Play).
fn volume_row(
    ui: &mut egui::Ui,
    browse_btn_width: f32,
    volume: &mut f32,
    sound_path: &str,
    builtin_sound: &'static [u8],
    preview_player: &mut SoundPreviewPlayer,
) {
    ui.label("Volume");
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let play_btn = egui::Button::new("Play").min_size(egui::vec2(browse_btn_width, 0.0));
        if ui.add(play_btn).clicked() {
            preview_player.play(sound_path, builtin_sound, *volume);
        }
        let mut pct = *volume * 100.0;
        if ui
            .add(
                egui::DragValue::new(&mut pct)
                    .range(0.0..=100.0)
                    .suffix("%")
                    .max_decimals(0),
            )
            .changed()
        {
            *volume = (pct / 100.0).clamp(0.0, 1.0);
        }
        let saved = ui.spacing().slider_width;
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(volume, 0.0..=1.0).show_value(false));
        ui.spacing_mut().slider_width = saved;
    });
    ui.end_row();
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

    /// Default valid params — tests override only the fields they care about.
    fn default_test_params(original: &Config) -> ValidateParams<'_> {
        ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            sound_enabled: true,
            mute_sound_volume: 1.0,
            unmute_sound_volume: 1.0,
            autostart: false,
            notifications_enabled: false,
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_command: "",
            on_unmute_command: "",
            original,
            max_sound_bytes: 10_000_000,
        }
    }

    #[test]
    fn build_valid_inputs_returns_ok() {
        let orig = Config::default();
        let config = build_and_validate_config(&default_test_params(&orig)).expect("should be Ok");
        assert_eq!(config.indicator.mute_color, "#FF0000");
        assert_eq!(config.keyboard.hotkey, "Ctrl+Shift+M");
        assert!(config.sound.sound_enabled);
        assert_eq!(config.sound.mute_sound_volume, 1.0);
        assert_eq!(config.sound.unmute_sound_volume, 1.0);
        assert!(!config.system.autostart);
        assert_eq!(config.indicator.mute_inputs, "all");
    }

    #[test]
    fn build_invalid_color_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Text,
            color_text: "not-a-color",
            color_rgb: [0.0, 0.0, 0.0],
            ..default_test_params(&orig)
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
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            hotkey: "",
            ..default_test_params(&orig)
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
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            hotkey: "Ctrl+Blah",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("Invalid hotkey")),
            "expected hotkey error, got: {errs:?}"
        );
    }

    #[test]
    fn build_picker_dirty_uses_rgb_conversion() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Picker,
            color_text: "garbage-text",
            color_rgb: [0.0, 1.0, 0.0],
            ..default_test_params(&orig)
        })
        .expect("picker dirty should use RGB, not text");
        assert_eq!(config.indicator.mute_color, "#00FF00");
    }

    #[test]
    fn build_preserves_original_fields() {
        let original = Config {
            indicator: focusmute_lib::config::IndicatorConfig {
                input_colors: HashMap::from([("1".into(), "#00FF00".into())]),
                ..Default::default()
            },
            system: focusmute_lib::config::SystemConfig {
                device_serial: "ABC123".to_string(),
                ..Default::default()
            },
            ..Config::default()
        };

        let config = build_and_validate_config(&ValidateParams {
            notifications_enabled: true,
            ..default_test_params(&original)
        })
        .expect("should be Ok");

        assert_eq!(config.system.device_serial, "ABC123");
        assert_eq!(config.indicator.input_colors.get("1").unwrap(), "#00FF00");
        // notifications_enabled comes from the form param, not original
        assert!(config.system.notifications_enabled);
    }

    #[test]
    fn build_hooks_are_preserved() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            on_mute_command: "echo muted",
            on_unmute_command: "echo unmuted",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");

        assert_eq!(config.hooks.on_mute_command, "echo muted");
        assert_eq!(config.hooks.on_unmute_command, "echo unmuted");
    }

    // NOTE: Color conversion tests (hex_to_rgb, rgb_to_hex, roundtrips) removed —
    // fully covered by led::color::tests in focusmute-lib.

    // ── T2: Additional settings dialog validation tests ──

    #[test]
    fn build_multiple_simultaneous_errors() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Text,
            color_text: "not-a-color",
            color_rgb: [0.0, 0.0, 0.0],
            hotkey: "",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.len() >= 2,
            "should collect multiple errors, got {}: {errs:?}",
            errs.len()
        );
        assert!(errs.iter().any(|e| e.to_lowercase().contains("color")));
        assert!(errs.iter().any(|e| e.to_lowercase().contains("hotkey")));
    }

    #[test]
    fn build_whitespace_only_color_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Text,
            color_text: "   ",
            color_rgb: [0.0, 0.0, 0.0],
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_lowercase().contains("color")),
            "expected color error, got: {errs:?}"
        );
    }

    #[test]
    fn build_picker_dirty_overrides_invalid_text() {
        // When picker is dirty, the RGB value is used even if color_text is invalid.
        // This tests that validation passes because the picker value is valid.
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Picker,
            color_text: "invalid",
            color_rgb: [0.5, 0.5, 0.5],
            ..default_test_params(&orig)
        });
        assert!(
            result.is_ok(),
            "picker dirty should use RGB, ignoring invalid text"
        );
        let config = result.unwrap();
        assert_eq!(config.indicator.mute_color, "#808080");
    }

    #[test]
    fn build_independent_sound_volumes() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            mute_sound_volume: 0.3,
            unmute_sound_volume: 0.8,
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert_eq!(config.sound.mute_sound_volume, 0.3);
        assert_eq!(config.sound.unmute_sound_volume, 0.8);
    }

    #[test]
    fn build_sound_volume_out_of_range_returns_err() {
        let orig = Config::default();
        for bad in [1.5, -0.1] {
            let result = build_and_validate_config(&ValidateParams {
                mute_sound_volume: bad,
                ..default_test_params(&orig)
            });
            assert!(
                result.is_err(),
                "mute_sound_volume {bad} should fail validation"
            );
            let errs = result.unwrap_err();
            assert!(
                errs.iter().any(|e| e.to_lowercase().contains("volume")),
                "expected volume error for {bad}, got: {errs:?}"
            );
        }
    }

    #[test]
    fn build_notifications_enabled_true_preserved() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            notifications_enabled: true,
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert!(config.system.notifications_enabled);
    }

    #[test]
    fn build_nan_sound_volume_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            mute_sound_volume: f32::NAN,
            ..default_test_params(&orig)
        });
        assert!(
            result.is_err(),
            "NaN mute_sound_volume should fail validation"
        );
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_lowercase().contains("volume")),
            "expected volume error for NaN, got: {errs:?}"
        );
    }

    #[test]
    fn build_text_dirty_uses_text_not_rgb() {
        // When color_dirty is Text and text is valid, the text value should be used
        // (not the RGB picker value).
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            color_dirty: &ColorDirty::Text,
            color_text: "#00FF00",
            color_rgb: [1.0, 0.0, 0.0], // red — should be ignored
            ..default_test_params(&orig)
        })
        .expect("valid text color should succeed");
        assert_eq!(config.indicator.mute_color, "#00FF00");
    }
}
