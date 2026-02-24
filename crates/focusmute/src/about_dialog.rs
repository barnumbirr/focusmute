//! About dialog — version and device info (cross-platform egui).

use focusmute_lib::device::DeviceInfo;

/// Show the About dialog.
///
/// Displays version, device info (if connected), and project URL.
/// Blocks until the user dismisses it.
///
/// Must be called from the main thread (eframe/winit requirement).
pub fn show_about(device_info: Option<&DeviceInfo>) {
    let version = env!("CARGO_PKG_VERSION");
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

    #[cfg(any(windows, target_os = "linux"))]
    {
        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([320.0, 270.0])
                .with_resizable(false)
                .with_title("About Focusmute")
                .with_icon(crate::icon::app_icon()),
            ..Default::default()
        };
        if let Err(e) = eframe::run_native(
            "About Focusmute",
            options,
            Box::new(move |cc| {
                // Apply same rounded widget style
                let mut style = (*cc.egui_ctx.style()).clone();
                let corner_radius = eframe::egui::CornerRadius::same(4);
                style.visuals.widgets.noninteractive.corner_radius = corner_radius;
                style.visuals.widgets.inactive.corner_radius = corner_radius;
                style.visuals.widgets.active.corner_radius = corner_radius;
                style.visuals.widgets.hovered.corner_radius = corner_radius;
                cc.egui_ctx.set_style(style);

                Ok(Box::new(AboutApp {
                    version: version.to_string(),
                    device_lines,
                    needs_resize: true,
                }))
            }),
        ) {
            log::error!("about dialog failed: {e}");
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = device_info;
        println!("Focusmute v{version}");
        for (key, val) in &device_lines {
            println!("{key}: {val}");
        }
        println!("\nSource: https://github.com/barnumbirr/focusmute");
    }
}

#[cfg(any(windows, target_os = "linux"))]
struct AboutApp {
    version: String,
    device_lines: Vec<(String, String)>,
    /// Resize the viewport to fit content after the first frame.
    needs_resize: bool,
}

#[cfg(any(windows, target_os = "linux"))]
impl eframe::App for AboutApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        use eframe::egui;

        // ── Bottom panel: pinned OK button ──
        egui::TopBottomPanel::bottom("about_buttons_panel")
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    let ok_btn = egui::Button::new("OK")
                        .fill(egui::Color32::from_rgb(60, 130, 210))
                        .min_size(egui::vec2(80.0, 0.0));
                    if ui.add(ok_btn).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.add_space(8.0);
            });

        // ── Central panel: content ──
        let mut content_bottom = 0.0_f32;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);

            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("Focusmute v{}", self.version))
                        .strong()
                        .size(18.0),
                );
                ui.add_space(4.0);
                ui.label("Hotkey mute control for Focusrite Scarlett 4th Gen interfaces");
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            egui::Grid::new("device_info_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for (key, val) in &self.device_lines {
                        ui.label(egui::RichText::new(format!("{key}:")).strong());
                        ui.label(val);
                        ui.end_row();
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("Source:");
                ui.hyperlink_to(
                    "github.com/barnumbirr/focusmute",
                    "https://github.com/barnumbirr/focusmute",
                );
            });

            content_bottom = ui.cursor().top();
        });

        // After the first frame, shrink the window to fit actual content
        // (content area + bottom panel height + margin).
        if self.needs_resize {
            self.needs_resize = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                320.0,
                content_bottom + 48.0,
            )));
        }
    }
}
