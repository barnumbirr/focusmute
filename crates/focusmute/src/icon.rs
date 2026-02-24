//! Shared application icon for egui dialog windows.

use eframe::egui;

pub(crate) const ICON_PNG: &[u8] = include_bytes!("../assets/icon-live.png");

/// Decode the embedded PNG into `egui::IconData` for use with `ViewportBuilder::with_icon`.
pub fn app_icon() -> egui::IconData {
    let img = image::load_from_memory(ICON_PNG)
        .expect("Failed to decode embedded icon PNG")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}
