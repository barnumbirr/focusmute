//! Cross-platform egui settings dialog.

use std::sync::{Arc, Mutex};

use eframe::egui;
use focusmute_lib::config::Config;
use focusmute_lib::led;

use super::{MAX_SOUND_FILE_BYTES, combo_to_mute_inputs, inputs_combo_items};

/// Tracks which side of the color sync last changed.
#[derive(PartialEq)]
enum ColorDirty {
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
    /// Previous About section openness — resize while animating.
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

            original: config,

            device_lines,

            errors: Vec::new(),

            result,

            needs_resize: true,
            prev_about_openness: -1.0,
        }
    }

    /// Try to save: validate, send result, and close on success.
    fn try_save(&mut self, ctx: &egui::Context) {
        let mute_inputs = combo_to_mute_inputs(self.mute_inputs_index, self.input_count);

        // Sync color from picker if that was the last change
        let color_str = if self.color_dirty == ColorDirty::Picker {
            rgb_to_hex(self.color_rgb)
        } else {
            self.color_text.clone()
        };

        let candidate = Config {
            mute_color: color_str,
            hotkey: self.hotkey.clone(),
            sound_enabled: self.sound_enabled,
            autostart: self.autostart,
            mute_inputs,
            mute_sound_path: self.mute_sound_path.clone(),
            unmute_sound_path: self.unmute_sound_path.clone(),
            device_serial: self.original.device_serial.clone(),
            on_mute_command: self.original.on_mute_command.clone(),
            on_unmute_command: self.original.on_unmute_command.clone(),
            input_colors: self.original.input_colors.clone(),
            notifications_enabled: self.original.notifications_enabled,
        };

        let input_count = if self.input_count > 0 {
            Some(self.input_count)
        } else {
            None
        };

        match candidate.validate(input_count, MAX_SOUND_FILE_BYTES) {
            Ok(()) => {
                *self.result.lock().unwrap() = Some(candidate);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(errs) => {
                self.errors = errs.iter().map(|e| e.to_string()).collect();
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
        let mut about_openness = 0.0_f32;
        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Mute Indicator section ──
            section_frame(ui, "Mute Indicator", |ui| {
                egui::Grid::new("mute_indicator_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        // Color row
                        ui.label("Mute Color");
                        ui.horizontal(|ui| {
                            let text_response = ui.add(
                                egui::TextEdit::singleline(&mut self.color_text)
                                    .desired_width(140.0),
                            );
                            if text_response.changed() {
                                self.color_dirty = ColorDirty::Text;
                                if let Some(rgb) = hex_to_rgb(&self.color_text) {
                                    self.color_rgb = rgb;
                                }
                            }

                            let before = self.color_rgb;
                            ui.color_edit_button_rgb(&mut self.color_rgb);
                            if self.color_rgb != before {
                                self.color_dirty = ColorDirty::Picker;
                                self.color_text = rgb_to_hex(self.color_rgb);
                            }
                        });
                        ui.end_row();

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

                // Compute responsive text field width: available frame width
                // minus label column (80), grid spacing (12), Browse button (~72),
                // and horizontal spacing (4).
                let text_width = (ui.available_width() - 80.0 - 12.0 - 76.0).max(100.0);

                egui::Grid::new("sound_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Mute Sound");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.mute_sound_path)
                                    .desired_width(text_width),
                            );
                            if ui.button("Browse...").clicked()
                                && let Some(path) = browse_wav_file()
                            {
                                self.mute_sound_path = path;
                            }
                        });
                        ui.end_row();

                        ui.label("Unmute Sound");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.unmute_sound_path)
                                    .desired_width(text_width),
                            );
                            if ui.button("Browse...").clicked()
                                && let Some(path) = browse_wav_file()
                            {
                                self.unmute_sound_path = path;
                            }
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
                                    egui::RichText::new(format!("Focusmute v{version}"))
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

        // Resize on the first frame and while the About section animates.
        // content_bottom is measured before the right-to-left button layout,
        // so it reflects actual content height and doesn't depend on window
        // size — no feedback loop.
        let openness_animating = (about_openness - self.prev_about_openness).abs() > 0.001;
        self.prev_about_openness = about_openness;

        if self.needs_resize || openness_animating {
            self.needs_resize = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                440.0,
                (content_bottom + BUTTON_AREA_HEIGHT).round(),
            )));
        }
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
