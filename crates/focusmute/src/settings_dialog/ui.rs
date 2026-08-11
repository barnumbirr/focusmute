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

/// Muted-talk blink sensitivity presets shown in the settings dialog,
/// mapped to raw `talk_threshold` meter values (higher sensitivity =
/// lower threshold). The raw `[indicator].talk_threshold` TOML key
/// remains the escape hatch for setups outside these presets.
const TALK_SENSITIVITY_PRESETS: &[(&str, u32)] = &[("Low", 500), ("Medium", 250), ("High", 100)];

/// Display text for the sensitivity combo: the preset name when the current
/// threshold matches one, otherwise "Custom (n)" — a hand-edited TOML value
/// is shown as-is and never silently replaced.
fn sensitivity_text(threshold: u32) -> String {
    TALK_SENSITIVITY_PRESETS
        .iter()
        .find(|(_, v)| *v == threshold)
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| format!("Custom ({threshold})"))
}

pub struct SettingsApp {
    // ── Form state ──
    color_text: String,
    color_rgb: [f32; 3],
    color_dirty: ColorDirty,

    hotkey: String,
    ptt_hotkey: String,

    mute_inputs_index: usize,
    mute_inputs_items: Vec<String>,
    input_count: usize,

    blink_on_talk: bool,
    talk_threshold: u32,

    sound_enabled: bool,
    suppress_browser_sync_sound: bool,
    mute_sound_volume: f32,
    unmute_sound_volume: f32,
    autostart: bool,
    notifications_enabled: bool,
    log_level: String,

    mute_sound_path: String,
    unmute_sound_path: String,

    on_mute_url: String,
    on_unmute_url: String,
    on_mute_body: String,
    on_unmute_body: String,

    browser_sync_port: String,
    browser_sync_reverse: bool,

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
        let mut style = (*cc.egui_ctx.global_style()).clone();
        let corner_radius = egui::CornerRadius::same(4);
        style.visuals.widgets.noninteractive.corner_radius = corner_radius;
        style.visuals.widgets.inactive.corner_radius = corner_radius;
        style.visuals.widgets.active.corner_radius = corner_radius;
        style.visuals.widgets.hovered.corner_radius = corner_radius;
        cc.egui_ctx.set_global_style(style);

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
            ptt_hotkey: config.keyboard.push_to_talk_hotkey.clone(),

            mute_inputs_index,
            mute_inputs_items,
            input_count,

            blink_on_talk: config.indicator.blink_on_talk,
            talk_threshold: config.indicator.talk_threshold,

            sound_enabled: config.sound.sound_enabled,
            suppress_browser_sync_sound: config.sound.suppress_browser_sync_sound,
            mute_sound_volume: config.sound.mute_sound_volume,
            unmute_sound_volume: config.sound.unmute_sound_volume,
            autostart: config.system.autostart,
            notifications_enabled: config.system.notifications_enabled,
            log_level: config.system.log_level.clone(),

            mute_sound_path: config.sound.mute_sound_path.clone(),
            unmute_sound_path: config.sound.unmute_sound_path.clone(),

            on_mute_url: config.hooks.on_mute_url.clone(),
            on_unmute_url: config.hooks.on_unmute_url.clone(),
            on_mute_body: config.hooks.on_mute_body.clone(),
            on_unmute_body: config.hooks.on_unmute_body.clone(),

            browser_sync_port: config.system.browser_sync_port.to_string(),
            browser_sync_reverse: config.system.browser_sync_reverse,

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
            ptt_hotkey: &self.ptt_hotkey,
            sound_enabled: self.sound_enabled,
            suppress_browser_sync_sound: self.suppress_browser_sync_sound,
            mute_sound_volume: self.mute_sound_volume,
            unmute_sound_volume: self.unmute_sound_volume,
            autostart: self.autostart,
            notifications_enabled: self.notifications_enabled,
            log_level: &self.log_level,
            mute_inputs_index: self.mute_inputs_index,
            input_count: self.input_count,
            mute_sound_path: &self.mute_sound_path,
            unmute_sound_path: &self.unmute_sound_path,
            on_mute_url: &self.on_mute_url,
            on_unmute_url: &self.on_unmute_url,
            on_mute_body: &self.on_mute_body,
            on_unmute_body: &self.on_unmute_body,
            browser_sync_port: &self.browser_sync_port,
            browser_sync_reverse: self.browser_sync_reverse,
            blink_on_talk: self.blink_on_talk,
            talk_threshold: self.talk_threshold,
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
            ptt_hotkey: self.ptt_hotkey.clone(),
            mute_inputs_index: self.mute_inputs_index,
            sound_enabled: self.sound_enabled,
            suppress_browser_sync_sound: self.suppress_browser_sync_sound,
            mute_sound_volume: self.mute_sound_volume,
            unmute_sound_volume: self.unmute_sound_volume,
            autostart: self.autostart,
            notifications_enabled: self.notifications_enabled,
            log_level: self.log_level.clone(),
            mute_sound_path: self.mute_sound_path.clone(),
            unmute_sound_path: self.unmute_sound_path.clone(),
            on_mute_url: self.on_mute_url.clone(),
            on_unmute_url: self.on_unmute_url.clone(),
            on_mute_body: self.on_mute_body.clone(),
            on_unmute_body: self.on_unmute_body.clone(),
            browser_sync_port: self.browser_sync_port.clone(),
            browser_sync_reverse: self.browser_sync_reverse,
            blink_on_talk: self.blink_on_talk,
            talk_threshold: self.talk_threshold,
        }
    }
}

/// All form fields snapshotted for change detection — no element-count limit.
#[derive(PartialEq)]
struct FormSnapshot {
    color_text: String,
    color_rgb: [f32; 3],
    hotkey: String,
    ptt_hotkey: String,
    mute_inputs_index: usize,
    sound_enabled: bool,
    suppress_browser_sync_sound: bool,
    mute_sound_volume: f32,
    unmute_sound_volume: f32,
    autostart: bool,
    notifications_enabled: bool,
    log_level: String,
    mute_sound_path: String,
    unmute_sound_path: String,
    on_mute_url: String,
    on_unmute_url: String,
    on_mute_body: String,
    on_unmute_body: String,
    browser_sync_port: String,
    browser_sync_reverse: bool,
    blink_on_talk: bool,
    talk_threshold: u32,
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.34: the root Ui replaces the Context parameter. Keep a
        // `ctx` binding so viewport commands and &Context helpers read as before.
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // Height of the button area below content (separator + padding + buttons).
        const BUTTON_AREA_HEIGHT: f32 = 54.0;

        // Snapshot form state before rendering — if anything changes,
        // clear stale validation errors so the Save button stays reachable.
        let form_snap = self.form_snapshot();

        let mut content_bottom = 0.0_f32;
        egui::CentralPanel::default().show_inside(ui, |ui| {
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

                        // Blink-on-talk row
                        ui.label("Blink on talk").on_hover_text(
                            "Blink the mute indicator when you talk while muted",
                        );
                        ui.checkbox(&mut self.blink_on_talk, "");
                        ui.end_row();

                        if self.blink_on_talk {
                            ui.label("Sensitivity").on_hover_text(
                                "How loud you need to be for the blink to trigger",
                            );
                            egui::ComboBox::from_id_salt("talk_sensitivity_combo")
                                .selected_text(sensitivity_text(self.talk_threshold))
                                .show_ui(ui, |ui| {
                                    for &(name, value) in TALK_SENSITIVITY_PRESETS {
                                        ui.selectable_value(
                                            &mut self.talk_threshold,
                                            value,
                                            name,
                                        );
                                    }
                                });
                            ui.end_row();
                        }
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

                        ui.label("Push-to-talk");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ptt_hotkey)
                                .desired_width(text_width)
                                .hint_text("e.g. Ctrl+Space (empty = off)"),
                        );
                        ui.end_row();
                    });
            });

            // ── Sound section ──
            section_frame(ui, "Sound", |ui| {
                ui.checkbox(&mut self.sound_enabled, "Sound feedback");
                ui.add_space(4.0);

                // Measure "Browse..." text width so Play buttons can match.
                let browse_text_width = ui.fonts_mut(|f| {
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
                ui.add_space(4.0);
                egui::Grid::new("system_grid")
                    .num_columns(2)
                    .min_col_width(80.0)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Log level");
                        egui::ComboBox::from_id_salt("log_level_combo")
                            .selected_text(&self.log_level)
                            .show_ui(ui, |ui| {
                                for &level in focusmute_lib::config::VALID_LOG_LEVELS {
                                    ui.selectable_value(
                                        &mut self.log_level,
                                        level.to_string(),
                                        level,
                                    );
                                }
                            });
                        ui.end_row();
                    });
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
                                ui.label(egui::RichText::new("Webhooks").strong());
                                ui.label("ℹ").on_hover_ui(|ui| {
                                    ui.label("HTTP POST sent on mute state changes. Body is optional — defaults to {\"event\":\"mute\"} / {\"event\":\"unmute\"}.");
                                });
                            });
                            ui.add_space(2.0);
                            ui.label("On mute URL");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_mute_url)
                                    .desired_width(text_width)
                                    .hint_text("https://example.com/webhook"),
                            );
                            ui.label("Body");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_mute_body)
                                    .desired_width(text_width)
                                    .hint_text(r#"{"event":"mute"}"#),
                            );
                            ui.add_space(4.0);
                            ui.label("On unmute URL");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_unmute_url)
                                    .desired_width(text_width)
                                    .hint_text("https://example.com/webhook"),
                            );
                            ui.label("Body");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.on_unmute_body)
                                    .desired_width(text_width)
                                    .hint_text(r#"{"event":"unmute"}"#),
                            );

                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Browser sync").strong());
                                ui.label("ℹ").on_hover_ui(|ui| {
                                    ui.label("Syncs mute state from browser-based meeting apps (Google Meet, Teams) via a browser extension.");
                                    ui.label("Install the FocusMute extension and set the same port here. Default: 9736. Requires restart.");
                                });
                            });
                            ui.add_space(2.0);
                            ui.checkbox(&mut self.suppress_browser_sync_sound, "Suppress sound on mute/unmute");
                            ui.add_space(2.0);
                            ui.checkbox(
                                &mut self.browser_sync_reverse,
                                "Let FocusMute mute/unmute the meeting",
                            )
                            .on_hover_text(
                                "Hotkey and tray mute changes click the meeting's own mute button (Google Meet, Microsoft Teams)",
                            );
                            ui.add_space(2.0);
                            egui::Grid::new("browser_sync_grid")
                                .num_columns(2)
                                .min_col_width(80.0)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Port");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.browser_sync_port)
                                            .desired_width(120.0)
                                            .hint_text("0 = disabled, e.g. 9736"),
                                    );
                                    ui.end_row();
                                });
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
                // The fill is fixed, so the text color must be too — the
                // theme's light-mode text is near-black and unreadable on
                // the blue fill.
                let save_btn =
                    egui::Button::new(egui::RichText::new("Save").color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(60, 130, 210))
                        .min_size(egui::vec2(80.0, 0.0));
                if ui.add(save_btn).clicked() {
                    self.try_save(ui.ctx());
                }

                // No custom fill: the theme's default button styling stays
                // readable in both light and dark mode.
                let cancel_btn = egui::Button::new("Cancel").min_size(egui::vec2(80.0, 0.0));
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
    pub ptt_hotkey: &'a str,
    pub sound_enabled: bool,
    pub suppress_browser_sync_sound: bool,
    pub mute_sound_volume: f32,
    pub unmute_sound_volume: f32,
    pub autostart: bool,
    pub notifications_enabled: bool,
    pub log_level: &'a str,
    pub mute_inputs_index: usize,
    pub input_count: usize,
    pub mute_sound_path: &'a str,
    pub unmute_sound_path: &'a str,
    pub on_mute_url: &'a str,
    pub on_unmute_url: &'a str,
    pub on_mute_body: &'a str,
    pub on_unmute_body: &'a str,
    pub browser_sync_port: &'a str,
    pub browser_sync_reverse: bool,
    pub blink_on_talk: bool,
    pub talk_threshold: u32,
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

    // Parse browser_sync_port from string (empty or "0" = disabled)
    let ws_port_str = p.browser_sync_port.trim();
    let browser_sync_port: u16 = if ws_port_str.is_empty() {
        0
    } else {
        match ws_port_str.parse::<u16>() {
            Ok(v) => v,
            Err(_) => {
                return Err(vec![format!(
                    "Invalid browser sync port \"{}\". Enter a number (0 = disabled, e.g. 9736)",
                    p.browser_sync_port
                )]);
            }
        }
    };

    let candidate = Config {
        indicator: focusmute_lib::config::IndicatorConfig {
            mute_color: color_str,
            mute_inputs,
            input_colors: p.original.indicator.input_colors.clone(),
            blink_on_talk: p.blink_on_talk,
            talk_threshold: p.talk_threshold,
        },
        keyboard: focusmute_lib::config::KeyboardConfig {
            hotkey: p.hotkey.to_string(),
            push_to_talk_hotkey: p.ptt_hotkey.trim().to_string(),
        },
        sound: focusmute_lib::config::SoundConfig {
            sound_enabled: p.sound_enabled,
            suppress_browser_sync_sound: p.suppress_browser_sync_sound,
            mute_sound_path: p.mute_sound_path.to_string(),
            unmute_sound_path: p.unmute_sound_path.to_string(),
            mute_sound_volume: p.mute_sound_volume,
            unmute_sound_volume: p.unmute_sound_volume,
        },
        system: focusmute_lib::config::SystemConfig {
            autostart: p.autostart,
            device_serial: p.original.system.device_serial.clone(),
            notifications_enabled: p.notifications_enabled,
            log_level: p.log_level.to_string(),
            browser_sync_port,
            browser_sync_reverse: p.browser_sync_reverse,
        },
        hooks: focusmute_lib::config::HooksConfig {
            on_mute_url: p.on_mute_url.to_string(),
            on_unmute_url: p.on_unmute_url.to_string(),
            on_mute_body: p.on_mute_body.to_string(),
            on_unmute_body: p.on_unmute_body.to_string(),
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
    let parsed_toggle = hotkey_str.parse::<global_hotkey::hotkey::HotKey>();
    if !hotkey_str.is_empty() && parsed_toggle.is_err() {
        errors.push("Invalid hotkey. Examples: Ctrl+Shift+M, Alt+F1".to_string());
    }

    // Validate PTT hotkey syntax (empty = disabled, which is fine)
    let ptt_str = p.ptt_hotkey.trim();
    if !ptt_str.is_empty() {
        match ptt_str.parse::<global_hotkey::hotkey::HotKey>() {
            Err(_) => {
                errors
                    .push("Invalid push-to-talk hotkey. Examples: Ctrl+Space, Alt+F2".to_string());
            }
            Ok(parsed_ptt) => {
                // Compare parsed hotkey IDs, not strings — catches reordered modifiers
                // like "Ctrl+Shift+M" vs "Shift+Ctrl+M".
                if parsed_toggle
                    .as_ref()
                    .is_ok_and(|t| t.id() == parsed_ptt.id())
                {
                    errors.push(
                        "Push-to-talk hotkey must be different from the toggle hotkey.".to_string(),
                    );
                }
            }
        }
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

    #[test]
    fn sensitivity_presets_are_valid_and_ordered() {
        // All presets within the 12-bit meter range, and "higher sensitivity"
        // means a strictly lower threshold.
        let values: Vec<u32> = TALK_SENSITIVITY_PRESETS.iter().map(|(_, v)| *v).collect();
        assert!(values.iter().all(|&v| v <= 4095));
        assert!(values.windows(2).all(|w| w[0] > w[1]));
        // The Medium preset is the shipped default.
        assert!(TALK_SENSITIVITY_PRESETS.contains(&("Medium", 250)));
    }

    #[test]
    fn sensitivity_text_names_presets_and_preserves_custom() {
        assert_eq!(sensitivity_text(500), "Low");
        assert_eq!(sensitivity_text(250), "Medium");
        assert_eq!(sensitivity_text(100), "High");
        assert_eq!(sensitivity_text(300), "Custom (300)");
    }

    /// Default valid params — tests override only the fields they care about.
    fn default_test_params(original: &Config) -> ValidateParams<'_> {
        ValidateParams {
            color_dirty: &ColorDirty::Neither,
            color_text: "#FF0000",
            color_rgb: [1.0, 0.0, 0.0],
            hotkey: "Ctrl+Shift+M",
            ptt_hotkey: "",
            browser_sync_reverse: false,
            blink_on_talk: false,
            talk_threshold: 250,
            sound_enabled: true,
            suppress_browser_sync_sound: true,
            mute_sound_volume: 1.0,
            unmute_sound_volume: 1.0,
            autostart: false,
            notifications_enabled: false,
            log_level: "info",
            mute_inputs_index: 0,
            input_count: 2,
            mute_sound_path: "",
            unmute_sound_path: "",
            on_mute_url: "",
            on_unmute_url: "",
            on_mute_body: "",
            on_unmute_body: "",
            browser_sync_port: "0",
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
            on_mute_url: "https://example.com/mute",
            on_unmute_url: "https://example.com/unmute",
            on_mute_body: r#"{"muted":true}"#,
            on_unmute_body: r#"{"muted":false}"#,
            ..default_test_params(&orig)
        })
        .expect("should be Ok");

        assert_eq!(config.hooks.on_mute_url, "https://example.com/mute");
        assert_eq!(config.hooks.on_unmute_url, "https://example.com/unmute");
        assert_eq!(config.hooks.on_mute_body, r#"{"muted":true}"#);
        assert_eq!(config.hooks.on_unmute_body, r#"{"muted":false}"#);
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

    // ── v0.7.4: user-friendly error messages ──

    #[test]
    fn build_invalid_hotkey_shows_examples() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            hotkey: "Not+A+Real+Key",
            ..default_test_params(&orig)
        });
        let errs = result.unwrap_err();
        let hotkey_err = errs.iter().find(|e| e.contains("hotkey")).unwrap();
        assert!(
            hotkey_err.contains("Examples"),
            "should show examples, got: {hotkey_err}"
        );
        assert!(
            hotkey_err.contains("Ctrl+Shift+M"),
            "should include Ctrl+Shift+M example, got: {hotkey_err}"
        );
    }

    #[test]
    fn build_valid_hotkey_no_error() {
        for hk in &["Ctrl+Shift+M", "Alt+F1", "F12", "Ctrl+M"] {
            let orig = Config::default();
            let result = build_and_validate_config(&ValidateParams {
                hotkey: hk,
                ..default_test_params(&orig)
            });
            assert!(result.is_ok(), "hotkey '{hk}' should be valid");
        }
    }

    #[test]
    fn build_ptt_hotkey_preserved_in_config() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            ptt_hotkey: "Ctrl+Space",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert_eq!(config.keyboard.push_to_talk_hotkey, "Ctrl+Space");
    }

    #[test]
    fn build_empty_ptt_hotkey_means_disabled() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            ptt_hotkey: "",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert!(config.keyboard.push_to_talk_hotkey.is_empty());
    }

    #[test]
    fn build_invalid_ptt_hotkey_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            ptt_hotkey: "Not+A+Real+Key",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("push-to-talk")),
            "expected PTT error, got: {errs:?}"
        );
    }

    #[test]
    fn build_ptt_same_as_toggle_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            hotkey: "Ctrl+Shift+M",
            ptt_hotkey: "Ctrl+Shift+M",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("different")),
            "expected duplicate hotkey error, got: {errs:?}"
        );
    }

    #[test]
    fn build_ptt_same_as_toggle_reordered_modifiers_returns_err() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            hotkey: "Ctrl+Shift+M",
            ptt_hotkey: "Shift+Ctrl+M",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("different")),
            "reordered modifiers should still detect duplicate, got: {errs:?}"
        );
    }

    #[test]
    fn build_ptt_whitespace_treated_as_disabled() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            ptt_hotkey: "   ",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert!(config.keyboard.push_to_talk_hotkey.is_empty());
    }

    // ── WebSocket port ──

    #[test]
    fn build_valid_browser_sync_port() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            browser_sync_port: "9736",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert_eq!(config.system.browser_sync_port, 9736);
    }

    #[test]
    fn build_browser_sync_port_zero_is_disabled() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            browser_sync_port: "0",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert_eq!(config.system.browser_sync_port, 0);
    }

    #[test]
    fn build_browser_sync_port_empty_is_disabled() {
        let orig = Config::default();
        let config = build_and_validate_config(&ValidateParams {
            browser_sync_port: "",
            ..default_test_params(&orig)
        })
        .expect("should be Ok");
        assert_eq!(config.system.browser_sync_port, 0);
    }

    #[test]
    fn build_browser_sync_port_invalid_string() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            browser_sync_port: "abc",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.contains("browser sync port")),
            "expected port parse error, got: {errs:?}"
        );
    }

    #[test]
    fn build_browser_sync_port_privileged_rejected() {
        let orig = Config::default();
        let result = build_and_validate_config(&ValidateParams {
            browser_sync_port: "80",
            ..default_test_params(&orig)
        });
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.to_lowercase().contains("privileged")),
            "expected privileged port error, got: {errs:?}"
        );
    }
}
