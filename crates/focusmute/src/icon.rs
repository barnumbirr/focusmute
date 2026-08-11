//! Shared application icon for egui dialog windows.

use eframe::egui;

pub(crate) const ICON_PNG: &[u8] = include_bytes!("../assets/icon-live.png");

/// Extract the embedded PNG to the config directory and return its path.
///
/// Used on Linux for `notify-rust` notification icons (parallel to the
/// Windows AUMID icon extraction in `tray/mod.rs`).
#[cfg(target_os = "linux")]
pub(crate) fn notification_icon_path() -> Option<String> {
    let dir = focusmute_lib::config::Config::dir()?;
    let path = dir.join("icon.png");
    if !path.exists() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, ICON_PNG);
    }
    Some(path.to_string_lossy().into_owned())
}

/// Decode the embedded ICO into `egui::IconData` for use with `ViewportBuilder::with_icon`.
///
/// Uses the same 32 px ICO entry as the tray icon so the crossbar is visible
/// at titlebar size.
pub fn app_icon() -> egui::IconData {
    use crate::tray::state::icon::{ICON_LIVE_ICO, TRAY_ICON_SIZE, decode_ico_entry};

    let img = decode_ico_entry(ICON_LIVE_ICO, TRAY_ICON_SIZE)
        .expect("Failed to decode embedded icon ICO")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}
