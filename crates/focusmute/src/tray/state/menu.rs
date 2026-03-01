//! Tray menu construction, notifications, and mute-state UI updates.

use focusmute_lib::config::Config;
use focusmute_lib::monitor::MonitorAction;

use muda::{Menu, MenuItem, PredefinedMenuItem};

use super::icon::{icon_live, icon_muted};
use super::{TrayResources, TrayState};
use crate::sound;

// ── Shared menu construction ──

/// All menu items the tray uses, returned from `build_tray_menu`.
pub struct TrayMenu {
    pub status_item: MenuItem,
    pub toggle_item: MenuItem,
    pub settings_item: MenuItem,
    pub reconnect_item: MenuItem,
    pub quit_item: MenuItem,
}

impl TrayMenu {
    /// Update menu state based on device connection status.
    pub fn set_device_connected(&self, connected: bool) {
        self.reconnect_item.set_enabled(!connected);
        self.status_item
            .set_text(if connected { "Live" } else { "Disconnected" });
    }
}

/// Build the tray context menu with all standard items.
pub fn build_tray_menu(config: &Config, initial_muted: bool) -> (Menu, TrayMenu) {
    let menu = Menu::new();
    let initial_status = if initial_muted { "Muted" } else { "Live" };
    let status_item = MenuItem::new(initial_status, false, None);
    let toggle_label = format!("Toggle Mute\t{}", config.hotkey);
    let toggle_item = MenuItem::new(&toggle_label, true, None);
    let settings_item = MenuItem::new("Settings...", true, None);
    let reconnect_item = MenuItem::new("Reconnect Device", false, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let _ = menu.append(&status_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&toggle_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&settings_item);
    let _ = menu.append(&reconnect_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit_item);

    (
        menu,
        TrayMenu {
            status_item,
            toggle_item,
            settings_item,
            reconnect_item,
            quit_item,
        },
    )
}

/// Build the tray icon with the correct initial state.
pub fn build_tray_icon(
    initial_muted: bool,
    menu: Menu,
) -> focusmute_lib::error::Result<tray_icon::TrayIcon> {
    let initial_tooltip = if initial_muted {
        "Focusmute — Muted"
    } else {
        "Focusmute — Live"
    };
    let initial_icon = if initial_muted {
        icon_muted()
    } else {
        icon_live()
    };
    tray_icon::TrayIconBuilder::new()
        .with_tooltip(initial_tooltip)
        .with_icon(initial_icon)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| {
            focusmute_lib::FocusmuteError::Config(format!("Failed to create tray icon: {e}"))
        })
}

/// Show startup warnings as a desktop notification.
///
/// Always shown regardless of `notifications_enabled` — if the config is broken,
/// that flag itself may be wrong.
pub(crate) fn show_startup_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    let body = warnings.join("\n");
    show_notification(&format!("Config warnings:\n{body}"));
}

/// Show a desktop notification with the given body text.
fn show_notification(body: &str) {
    let mut n = notify_rust::Notification::new();
    #[cfg(windows)]
    n.app_id(crate::tray::AUMID);
    #[cfg(target_os = "linux")]
    n.summary("Focusmute");
    n.body(body);
    let _ = n.show();
}

/// Apply mute-state UI updates to the tray icon and status item.
pub fn apply_mute_ui(
    action: MonitorAction,
    tray: &tray_icon::TrayIcon,
    menu: &TrayMenu,
    state: &TrayState,
    resources: &TrayResources,
) {
    match action {
        MonitorAction::ApplyMute => {
            tray.set_icon(Some(icon_muted())).ok();
            tray.set_tooltip(Some("Focusmute — Muted")).ok();
            menu.status_item.set_text("Muted");
            if state.config.sound_enabled
                && let Some(ref s) = resources.sink
            {
                sound::play_sound(&resources.mute_sound, s);
            }
            if state.config.notifications_enabled {
                show_notification("Microphone Muted");
            }
        }
        MonitorAction::ClearMute => {
            tray.set_icon(Some(icon_live())).ok();
            tray.set_tooltip(Some("Focusmute — Live")).ok();
            menu.status_item.set_text("Live");
            if state.config.sound_enabled
                && let Some(ref s) = resources.sink
            {
                sound::play_sound(&resources.unmute_sound, s);
            }
            if state.config.notifications_enabled {
                show_notification("Microphone Live");
            }
        }
        MonitorAction::NoChange => {}
    }
    focusmute_lib::hooks::run_action_hook(action, &state.config);
}
