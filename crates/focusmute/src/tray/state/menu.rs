//! Tray menu construction, notifications, and mute-state UI updates.

use focusmute_lib::config::Config;
use focusmute_lib::monitor::MonitorAction;

use muda::{Menu, MenuItem, PredefinedMenuItem};

use super::icon::{icon_disconnected, icon_live, icon_muted};
use super::{TrayResources, TrayState};
use crate::sound;

// ── Tray icon helpers ──

/// Update the tray icon and tooltip in one call (both are best-effort).
pub fn set_tray_mute_state(tray: &tray_icon::TrayIcon, muted: bool) {
    if muted {
        tray.set_icon(Some(icon_muted())).ok();
        tray.set_tooltip(Some("FocusMute — Muted")).ok();
    } else {
        tray.set_icon(Some(icon_live())).ok();
        tray.set_tooltip(Some("FocusMute — Live")).ok();
    }
}

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
    /// Update the reconnect menu item label based on device connection status.
    pub fn set_reconnect_label(&self, connected: bool) {
        self.reconnect_item.set_text(if connected {
            "Re-sync device"
        } else {
            "Reconnect device"
        });
    }

    /// Update menu and tray icon based on device connection status.
    pub fn set_device_connected(&self, connected: bool, tray: &tray_icon::TrayIcon) {
        self.status_item.set_text(if connected {
            "Live"
        } else {
            "Connect a Scarlett 4th Gen device"
        });
        self.set_reconnect_label(connected);
        if !connected {
            tray.set_icon(Some(icon_disconnected())).ok();
            tray.set_tooltip(Some("FocusMute — Disconnected")).ok();
        }
    }
}

/// Build the tray context menu with all standard items.
pub fn build_tray_menu(config: &Config, initial_muted: bool) -> (Menu, TrayMenu) {
    let menu = Menu::new();
    let initial_status = if initial_muted { "Muted" } else { "Live" };
    let status_item = MenuItem::new(initial_status, false, None);
    let toggle_label = format!("Toggle Mute\t{}", config.keyboard.hotkey);
    let toggle_item = MenuItem::new(&toggle_label, true, None);
    let settings_item = MenuItem::new("Settings...", true, None);
    let reconnect_item = MenuItem::new("Reconnect device", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let append = |item: &dyn muda::IsMenuItem| {
        if let Err(e) = menu.append(item) {
            log::warn!("[menu] could not append menu item: {e}");
        }
    };
    append(&status_item);
    append(&PredefinedMenuItem::separator());
    append(&toggle_item);
    append(&PredefinedMenuItem::separator());
    append(&settings_item);
    append(&reconnect_item);
    append(&PredefinedMenuItem::separator());
    append(&quit_item);

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
    device_connected: bool,
    menu: Menu,
) -> focusmute_lib::error::Result<tray_icon::TrayIcon> {
    let (initial_tooltip, initial_icon) = if !device_connected {
        ("FocusMute — Disconnected", icon_disconnected())
    } else if initial_muted {
        ("FocusMute — Muted", icon_muted())
    } else {
        ("FocusMute — Live", icon_live())
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
    crate::notification::Notifier::show_oneshot(&format!("Config warnings:\n{body}"));
}

/// Apply mute-state UI updates to the tray icon and status item.
///
/// When `device_connected` is false, all UI/sound/notification updates are
/// suppressed — the audio interface is unplugged so the input states are
/// meaningless.
pub fn apply_mute_ui(
    action: MonitorAction,
    tray: &tray_icon::TrayIcon,
    menu: &TrayMenu,
    state: &TrayState,
    resources: &mut TrayResources,
    device_connected: bool,
    suppress_sound: bool,
) {
    if !device_connected {
        return;
    }
    let play_sound = state.config.sound.sound_enabled && !suppress_sound;
    match action {
        MonitorAction::ApplyMute => {
            log::info!("[mute] muted");
            set_tray_mute_state(tray, true);
            menu.status_item.set_text("Muted");
            if play_sound {
                sound::play_sound(&resources.mute_sound, state.config.sound.mute_sound_volume);
            }
            if state.config.system.notifications_enabled {
                resources.notifier.show_mute_state("Microphone Muted");
            }
        }
        MonitorAction::ClearMute => {
            log::info!("[mute] unmuted");
            set_tray_mute_state(tray, false);
            menu.status_item.set_text("Live");
            if play_sound {
                sound::play_sound(
                    &resources.unmute_sound,
                    state.config.sound.unmute_sound_volume,
                );
            }
            if state.config.system.notifications_enabled {
                resources.notifier.show_mute_state("Microphone Live");
            }
        }
        MonitorAction::NoChange => {}
    }
    focusmute_lib::hooks::run_action_hook(action, &state.config);
}
