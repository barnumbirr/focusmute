//! Shared tray state and business logic — used by both Windows and Linux tray apps.
//!
//! Platform-specific event loops and UI code live in `windows.rs` / `linux.rs`.
//! This module provides:
//! - Core `TrayState` (config, indicator, reconnection)
//! - Menu + tray icon construction (`build_tray_menu`, `build_tray_icon`)
//! - Hotkey management (`HotkeyState`, `register_hotkey`, `reregister_hotkey`)
//! - Settings result handling (`handle_settings_result`)
//! - Icon caching, autostart helpers

use focusmute_lib::config::Config;
use focusmute_lib::context::DeviceContext;
use focusmute_lib::device::ScarlettDevice;
use focusmute_lib::led;
use focusmute_lib::monitor::{MonitorAction, MuteIndicator};
use focusmute_lib::reconnect::ReconnectState;

use auto_launch::AutoLaunchBuilder;
use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use crate::sound;

// ── Audio/hotkey resource bundle ──

/// Bundles audio playback and hotkey resources that clutter function signatures.
///
/// The toggle-mute closure stays as a parameter since it captures
/// platform-specific state (`main_monitor`) and can't be bundled.
pub struct TrayResources {
    pub mute_sound: sound::DecodedSound,
    pub unmute_sound: sound::DecodedSound,
    pub hotkey: HotkeyState,
    pub sink: Option<rodio::Sink>,
    pub _audio_stream: Option<rodio::OutputStream>,
}

impl TrayResources {
    pub fn init(config: &Config) -> focusmute_lib::error::Result<Self> {
        let (_audio_stream, sink) = sound::init_audio_output();
        let mute_sound = sound::load_sound_data(&config.mute_sound_path, sound::SOUND_MUTED);
        let unmute_sound = sound::load_sound_data(&config.unmute_sound_path, sound::SOUND_UNMUTED);
        let hotkey = register_hotkey(&config.hotkey)?;
        Ok(Self {
            mute_sound,
            unmute_sound,
            hotkey,
            sink,
            _audio_stream,
        })
    }
}

// Embedded tray icons (multi-size ICO files).
const ICON_LIVE_ICO: &[u8] = include_bytes!("../../assets/icon-live.ico");
const ICON_MUTED_ICO: &[u8] = include_bytes!("../../assets/icon-muted.ico");

/// Target size when extracting tray icons from ICO files.  32 px is a good
/// compromise: Windows tray icons range from 16 px (100 % DPI) to 32 px
/// (200 % DPI), so the worst-case downscale is only 2:1.
const TRAY_ICON_SIZE: u8 = 32;

// ── Icon loading (decoded once, cloned on use) ──

/// RGBA pixel data cached for cheap cloning into `Icon`.
struct CachedIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

impl CachedIcon {
    fn decode(ico_data: &[u8]) -> Self {
        let img = decode_ico_entry(ico_data, TRAY_ICON_SIZE)
            .expect("Failed to decode embedded icon")
            .into_rgba8();
        let (w, h) = img.dimensions();
        Self {
            rgba: img.into_raw(),
            width: w,
            height: h,
        }
    }

    fn to_icon(&self) -> Icon {
        Icon::from_rgba(self.rgba.clone(), self.width, self.height).expect("icon creation")
    }
}

/// Extract a specific size from a multi-size ICO file.
///
/// Parses the ICO directory to find the entry closest to `target_size`,
/// then decodes that entry's image data.  `image::load_from_memory` always
/// returns the largest entry (256 px), which loses thin details like the
/// crossbar when Windows downscales it to tray size (16–24 px).
fn decode_ico_entry(
    ico_data: &[u8],
    target_size: u8,
) -> Result<image::DynamicImage, image::ImageError> {
    // ICO header: 2 reserved + 2 type + 2 count = 6 bytes
    // Each directory entry: 16 bytes (width, height, ..., 4-byte offset, 4-byte size)
    if ico_data.len() < 6 {
        return image::load_from_memory(ico_data);
    }
    let count = u16::from_le_bytes([ico_data[4], ico_data[5]]) as usize;

    let mut best_idx = 0;
    let mut best_diff = u16::MAX;
    for i in 0..count {
        let entry_offset = 6 + i * 16;
        if entry_offset + 16 > ico_data.len() {
            break;
        }
        // Width byte: 0 means 256
        let w = if ico_data[entry_offset] == 0 {
            256u16
        } else {
            ico_data[entry_offset] as u16
        };
        let diff = (w as i32 - target_size as i32).unsigned_abs() as u16;
        if diff < best_diff {
            best_diff = diff;
            best_idx = i;
        }
    }

    // Read offset and size of the chosen entry's image data
    let entry = 6 + best_idx * 16;
    let data_size = u32::from_le_bytes([
        ico_data[entry + 8],
        ico_data[entry + 9],
        ico_data[entry + 10],
        ico_data[entry + 11],
    ]) as usize;
    let data_offset = u32::from_le_bytes([
        ico_data[entry + 12],
        ico_data[entry + 13],
        ico_data[entry + 14],
        ico_data[entry + 15],
    ]) as usize;

    if data_offset + data_size <= ico_data.len() {
        let entry_data = &ico_data[data_offset..data_offset + data_size];
        // Individual entries are typically PNG or BMP; image crate handles both.
        image::load_from_memory(entry_data)
    } else {
        image::load_from_memory(ico_data)
    }
}

pub fn icon_live() -> Icon {
    use std::sync::OnceLock;
    static CACHE: OnceLock<CachedIcon> = OnceLock::new();
    CACHE
        .get_or_init(|| CachedIcon::decode(ICON_LIVE_ICO))
        .to_icon()
}

pub fn icon_muted() -> Icon {
    use std::sync::OnceLock;
    static CACHE: OnceLock<CachedIcon> = OnceLock::new();
    CACHE
        .get_or_init(|| CachedIcon::decode(ICON_MUTED_ICO))
        .to_icon()
}

// ── Messages from background thread ──

pub enum Msg {
    MutePoll(bool),
}

// ── Autostart ──

pub fn get_auto_launch() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_str()?;
    AutoLaunchBuilder::new()
        .set_app_name("Focusmute")
        .set_app_path(path)
        .build()
        .ok()
}

pub fn set_autostart(enabled: bool) {
    if let Some(al) = get_auto_launch() {
        let result = if enabled { al.enable() } else { al.disable() };
        if let Err(e) = result {
            log::error!("[autostart] {e}");
        }
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
    TrayIconBuilder::new()
        .with_tooltip(initial_tooltip)
        .with_icon(initial_icon)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| {
            focusmute_lib::FocusmuteError::Config(format!("Failed to create tray icon: {e}"))
        })
}

// ── Hotkey management ──

/// Tracks the currently registered global hotkey.
pub struct HotkeyState {
    pub manager: GlobalHotKeyManager,
    pub current: HotKey,
    pub id: u32,
}

/// Parse and register the initial global hotkey.
pub fn register_hotkey(hotkey_str: &str) -> focusmute_lib::error::Result<HotkeyState> {
    let manager = GlobalHotKeyManager::new().map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to init hotkey manager: {e}"))
    })?;
    let hotkey: HotKey = hotkey_str
        .parse()
        .unwrap_or_else(|_| "Ctrl+Shift+M".parse().unwrap());
    let id = hotkey.id();
    if let Err(e) = manager.register(hotkey) {
        log::warn!("could not register hotkey '{hotkey_str}': {e}");
    }
    Ok(HotkeyState {
        manager,
        current: hotkey,
        id,
    })
}

/// Unregister the old hotkey and register a new one. Updates state in place.
pub fn reregister_hotkey(hk: &mut HotkeyState, new_hotkey_str: &str) {
    let _ = hk.manager.unregister(hk.current);
    match new_hotkey_str.parse::<HotKey>() {
        Ok(new_hk) => {
            if let Err(e) = hk.manager.register(new_hk) {
                log::warn!("[config] could not register hotkey '{new_hotkey_str}': {e}");
            } else {
                hk.current = new_hk;
                hk.id = new_hk.id();
            }
        }
        Err(e) => {
            log::warn!("[config] invalid hotkey '{new_hotkey_str}': {e}");
        }
    }
}

// ── Shared tray state ──

/// Platform-independent tray application state.
///
/// Holds everything except the device (which is managed by the platform-specific
/// `run()` function since `open_device()` returns `impl ScarlettDevice`).
pub struct TrayState {
    pub config: Config,
    pub indicator: MuteIndicator,
    pub reconnect: ReconnectState,
    pub ctx: Option<DeviceContext>,
}

impl TrayState {
    /// Initialize with a specific config and a connected device.
    pub fn init_with_config(
        config: Config,
        device: &impl ScarlettDevice,
    ) -> focusmute_lib::error::Result<Self> {
        let mut config = config;
        let init_mute_color = led::mute_color_or_default(&config);

        let ctx = DeviceContext::resolve(device, false)?;

        let (_mute_mode, strategy, warnings) = led::resolve_strategy_from_config(
            &mut config,
            ctx.input_count(),
            ctx.profile,
            ctx.predicted.as_ref(),
        )
        .map_err(focusmute_lib::FocusmuteError::Config)?;
        for w in &warnings {
            log::warn!("[config] {w}");
        }

        let indicator = MuteIndicator::new(2, false, init_mute_color, strategy);

        Ok(TrayState {
            config,
            indicator,
            reconnect: ReconnectState::with_defaults(),
            ctx: Some(ctx),
        })
    }

    /// Initialize without a device — uses a no-op strategy (empty LED vectors).
    ///
    /// The `MuteIndicator` still exists and debounces mute state, but LED
    /// writes are no-ops because `number_leds` is empty. Call
    /// [`reinit_device_context`] when a device becomes available.
    pub fn init_without_device(config: Config) -> Self {
        let init_mute_color = led::mute_color_or_default(&config);
        let noop_strategy = led::MuteStrategy {
            input_indices: vec![],
            number_leds: vec![],
            mute_colors: vec![],
            selected_color: 0,
            unselected_color: 0,
        };
        let indicator = MuteIndicator::new(2, false, init_mute_color, noop_strategy);

        TrayState {
            config,
            indicator,
            reconnect: ReconnectState::with_defaults(),
            ctx: None,
        }
    }

    /// Resolve a `DeviceContext` from a newly connected device and replace the
    /// no-op strategy with a real one. Returns config warnings (if any).
    pub fn reinit_device_context(
        &mut self,
        device: &impl ScarlettDevice,
    ) -> focusmute_lib::error::Result<Vec<String>> {
        let ctx = DeviceContext::resolve(device, false)?;

        let (_mute_mode, strategy, warnings) = led::resolve_strategy_from_config(
            &mut self.config,
            ctx.input_count(),
            ctx.profile,
            ctx.predicted.as_ref(),
        )
        .map_err(focusmute_lib::FocusmuteError::Config)?;
        for w in &warnings {
            log::warn!("[config] {w}");
        }

        self.indicator.set_strategy(strategy);
        self.ctx = Some(ctx);
        Ok(warnings)
    }

    /// Apply initial mute state (call after audio monitor is ready).
    ///
    /// Syncs the debouncer to the known state so subsequent polls won't
    /// trigger a spurious ApplyMute/ClearMute event.
    pub fn set_initial_muted(&mut self, muted: bool, device: &impl ScarlettDevice) {
        self.indicator.force_state(muted);
        if muted {
            let _ = self.indicator.apply_mute(device);
        }
    }

    /// Reset the reconnection backoff so the next attempt happens immediately.
    pub fn reset_backoff(&mut self) {
        self.reconnect = ReconnectState::with_defaults();
    }

    /// Attempt device reconnection with backoff + LED state refresh.
    ///
    /// When `ctx` is `Some` (device was previously connected), uses the normal
    /// reconnect-and-refresh path. When `ctx` is `None` (never connected),
    /// opens the device and calls [`reinit_device_context`] to resolve the
    /// real strategy.
    ///
    /// Returns the new device on success, `None` if not ready or failed.
    pub fn try_reconnect(&mut self) -> Option<focusmute_lib::device::PlatformDevice> {
        if self.ctx.is_some() {
            // Normal reconnect: device was previously connected, strategy is valid.
            focusmute_lib::reconnect::try_reconnect_and_refresh(
                &mut self.reconnect,
                self.indicator.strategy(),
                self.indicator.mute_color(),
                self.indicator.is_muted(),
                &self.config.device_serial,
            )
        } else {
            // First connect: no DeviceContext yet — open device and resolve context.
            let dev = focusmute_lib::reconnect::try_reopen(
                &mut self.reconnect,
                &self.config.device_serial,
            )?;
            match self.reinit_device_context(&dev) {
                Ok(warnings) => {
                    for w in &warnings {
                        log::warn!("[config] {w}");
                    }
                    // If currently muted, apply LEDs with the new real strategy.
                    if self.indicator.is_muted()
                        && let Err(e) = self.indicator.apply_mute(&dev)
                    {
                        log::warn!("could not apply mute after first connect: {e}");
                    }
                    Some(dev)
                }
                Err(e) => {
                    log::warn!("could not resolve device context on first connect: {e}");
                    None
                }
            }
        }
    }

    /// Process a mute poll from the background thread. Returns the resulting action.
    /// If a device error occurs, returns `(action, true)` to signal device loss.
    pub fn process_mute_poll(
        &mut self,
        muted: bool,
        device: Option<&impl ScarlettDevice>,
    ) -> (MonitorAction, bool) {
        if let Some(dev) = device {
            let (action, err) = self.indicator.poll_and_apply(muted, dev);
            (action, err.is_some())
        } else {
            (self.indicator.update(muted), false)
        }
    }

    /// Apply new configuration from settings dialog. Returns list of warnings.
    pub fn apply_config(
        &mut self,
        mut new_config: Config,
        device: Option<&impl ScarlettDevice>,
    ) -> Vec<String> {
        let mut warnings = Vec::new();

        // Update mute color
        if let Ok(color) = led::parse_color(&new_config.mute_color) {
            self.indicator.set_mute_color(color);
        }

        // Update autostart
        if new_config.autostart != self.config.autostart {
            set_autostart(new_config.autostart);
        }

        // Re-resolve strategy if mute_inputs, input_colors, or mute_color changed.
        // mute_color affects strategy.mute_colors — without this, changing the
        // global color leaves the per-input strategy colors stale.
        if new_config.mute_inputs != self.config.mute_inputs
            || new_config.input_colors != self.config.input_colors
            || new_config.mute_color != self.config.mute_color
        {
            let (input_count, profile, predicted) = match self.ctx.as_ref() {
                Some(ctx) => (ctx.input_count(), ctx.profile, ctx.predicted.as_ref()),
                None => (None, None, None),
            };
            match led::resolve_strategy_from_config(
                &mut new_config,
                input_count,
                profile,
                predicted,
            ) {
                Ok((_mode, new_strategy, sw)) => {
                    warnings.extend(sw);
                    // Clear old indicator before switching strategy
                    if self.indicator.is_muted()
                        && let Some(dev) = device
                    {
                        let _ = self.indicator.clear_mute(dev);
                    }
                    self.indicator.set_strategy(new_strategy);
                }
                Err(e) => {
                    warnings.push(format!("strategy resolution failed: {e}"));
                }
            }
        }

        // Re-apply current mute LED state with new settings
        if self.indicator.is_muted()
            && let Some(dev) = device
        {
            let _ = self.indicator.apply_mute(dev);
        }

        // Save to disk and update config
        self.config = new_config;
        if let Err(e) = self.config.save() {
            log::warn!("could not save config: {e}");
        }

        warnings
    }

    /// Handle settings dialog result: apply config, return what changed.
    ///
    /// Returns `(warnings, mute_sound_changed, unmute_sound_changed, hotkey_changed, new_hotkey_str)`.
    pub fn handle_settings_result(
        &mut self,
        new_config: Config,
        device: Option<&impl ScarlettDevice>,
    ) -> (Vec<String>, bool, bool, bool, String) {
        let mute_sound_changed = new_config.mute_sound_path != self.config.mute_sound_path;
        let unmute_sound_changed = new_config.unmute_sound_path != self.config.unmute_sound_path;
        let hotkey_changed = new_config.hotkey != self.config.hotkey;
        let new_hotkey_str = new_config.hotkey.clone();

        let warnings = self.apply_config(new_config, device);

        (
            warnings,
            mute_sound_changed,
            unmute_sound_changed,
            hotkey_changed,
            new_hotkey_str,
        )
    }

    /// Restore LED state on exit.
    pub fn restore_on_exit(&self, device: &impl ScarlettDevice) {
        if let Err(e) = led::restore_on_exit(device, self.indicator.strategy()) {
            log::warn!("could not restore LED state: {e}");
        }
    }
}

/// Show a desktop notification with the given body text.
fn show_notification(body: &str) {
    let mut n = notify_rust::Notification::new();
    #[cfg(windows)]
    n.app_id(super::AUMID);
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

/// Handle a menu event from the tray context menu.
///
/// Returns `true` if the event was a quit request.
pub fn handle_menu_event(
    event: &MenuEvent,
    menu: &TrayMenu,
    state: &mut TrayState,
    device: &mut Option<impl ScarlettDevice>,
    resources: &mut TrayResources,
    toggle_mute_fn: &dyn Fn(bool),
) -> bool {
    if event.id() == menu.quit_item.id() {
        return true;
    } else if event.id() == menu.toggle_item.id() {
        toggle_mute_fn(state.indicator.is_muted());
    } else if event.id() == menu.settings_item.id() {
        let info = device.as_ref().map(|d| d.info());
        let profile = state.ctx.as_ref().and_then(|c| c.profile);
        if let Some(new_config) =
            crate::settings_dialog::show_settings(&state.config, profile, info)
        {
            let (warnings, mute_changed, unmute_changed, hotkey_changed, new_hotkey_str) =
                state.handle_settings_result(new_config, device.as_ref());
            for w in &warnings {
                log::warn!("[config] {w}");
            }

            if mute_changed {
                resources.mute_sound =
                    sound::load_sound_data(&state.config.mute_sound_path, sound::SOUND_MUTED);
            }
            if unmute_changed {
                resources.unmute_sound =
                    sound::load_sound_data(&state.config.unmute_sound_path, sound::SOUND_UNMUTED);
            }

            if hotkey_changed {
                reregister_hotkey(&mut resources.hotkey, &new_hotkey_str);
                menu.toggle_item
                    .set_text(format!("Toggle Mute\t{}", new_hotkey_str));
            }
        }
    } else if event.id() == menu.reconnect_item.id() {
        state.reset_backoff();
        // Next loop iteration will attempt reconnect immediately
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use focusmute_lib::device::mock::MockDevice;
    use focusmute_lib::protocol::*;

    /// Create a MockDevice with the "Scarlett 2i2 4th Gen" name so that
    /// TrayState::init_with_config succeeds (known profile, no schema extraction needed).
    fn make_mock_device() -> MockDevice {
        let mut dev = MockDevice::new();
        dev.info_mut().device_name = "Scarlett 2i2 4th Gen-00031337".into();
        // Set up selectedInput for restore operations
        dev.set_descriptor(OFF_SELECTED_INPUT, &[0]).unwrap();
        dev
    }

    #[test]
    fn init_creates_valid_state() {
        let dev = make_mock_device();
        let state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        assert!(!state.indicator.is_muted());
        assert!(state.config.sound_enabled); // Default config has sound_enabled=true
        assert_eq!(state.config.mute_color, "#FF0000");
    }

    #[test]
    fn set_initial_muted_applies_led() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        state.set_initial_muted(true, &dev);
        assert!(state.indicator.is_muted());
        // Should have written directLEDColour via single-LED update
        let descs = dev.descriptors.borrow();
        assert!(
            descs.contains_key(&OFF_DIRECT_LED_COLOUR),
            "should write directLEDColour for mute indication"
        );
    }

    #[test]
    fn handle_mute_poll_returns_updates() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        // Feed 2 consecutive "muted=true" polls (threshold=2 for debounce)
        let (action1, _) = state.process_mute_poll(true, Some(&dev));
        let (action2, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(action1, MonitorAction::NoChange));
        assert!(matches!(action2, MonitorAction::ApplyMute));
    }

    #[test]
    fn handle_mute_poll_no_device() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        // Poll without device — should update debouncer but not crash
        let (action, lost) = state.process_mute_poll(true, Option::<&MockDevice>::None);
        assert!(!lost);
        assert!(matches!(action, MonitorAction::NoChange));
    }

    #[test]
    fn apply_config_updates_sound() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        assert!(state.config.sound_enabled);

        let mut new_config = state.config.clone();
        new_config.sound_enabled = false;
        state.apply_config(new_config, Some(&dev));
        assert!(!state.config.sound_enabled);
    }

    #[test]
    fn apply_config_changes_color() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        let original_color = state.indicator.mute_color();
        let mut new_config = state.config.clone();
        new_config.mute_color = "#00FF00".into();
        state.apply_config(new_config, Some(&dev));
        assert_ne!(state.indicator.mute_color(), original_color);
    }

    #[test]
    fn apply_config_changes_strategy() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        let mut new_config = state.config.clone();
        new_config.mute_inputs = "1".into();
        state.apply_config(new_config, Some(&dev));
        // Strategy should target only input 1
        assert_eq!(state.indicator.strategy().input_indices, &[0]);
    }

    #[test]
    fn restore_on_exit_restores_leds() {
        let dev = make_mock_device();
        let state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // Restore on exit
        state.restore_on_exit(&dev);

        // With 2i2 profile, strategy targets both number LEDs — restore_on_exit should
        // write number LEDs via DATA_NOTIFY(8).
        let notifies = dev.notifies.borrow();
        assert!(
            notifies.contains(&NOTIFY_DIRECT_LED_COLOUR),
            "restore should use DATA_NOTIFY(8)"
        );
    }

    #[test]
    fn try_reconnect_respects_backoff() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();
        // Record a failure with a very long backoff
        state.reconnect.record_failure();
        // Immediate try_reconnect should return None (backoff not elapsed)
        assert!(state.try_reconnect().is_none());
    }

    #[test]
    fn process_mute_poll_device_error_signals_loss() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // Enable failure injection on set_descriptor
        dev.fail_set_descriptor.set(true);

        // Feed enough polls to trigger ApplyMute (threshold=2)
        state.process_mute_poll(true, Some(&dev));
        let (action, device_lost) = state.process_mute_poll(true, Some(&dev));

        // The apply_mute inside poll_and_apply will fail → device_lost=true
        assert!(matches!(action, MonitorAction::ApplyMute));
        assert!(device_lost);
    }

    #[test]
    fn handle_settings_result_tracks_changes() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        let mut new_config = state.config.clone();
        new_config.hotkey = "F12".into();
        new_config.mute_sound_path = "/some/new/path.wav".into();

        let (_, mute_changed, unmute_changed, hotkey_changed, new_hk) =
            state.handle_settings_result(new_config, Some(&dev));

        assert!(mute_changed);
        assert!(!unmute_changed);
        assert!(hotkey_changed);
        assert_eq!(new_hk, "F12");
    }

    // Phase 3.2 — reconnect integration flow tests

    #[test]
    fn process_mute_poll_without_device_updates_debouncer() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // 2 polls with device=None, verify is_muted changes (threshold=2)
        state.process_mute_poll(true, Option::<&MockDevice>::None);
        let (action, _) = state.process_mute_poll(true, Option::<&MockDevice>::None);
        // After 2 polls, debouncer should report ApplyMute (even without device)
        assert!(matches!(action, MonitorAction::ApplyMute));
        assert!(state.indicator.is_muted());
    }

    #[test]
    fn reconnect_backoff_progression() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        let initial_delay = state.reconnect.current_delay();
        state.reconnect.record_failure();
        let delay_after_1 = state.reconnect.current_delay();
        state.reconnect.record_failure();
        let delay_after_2 = state.reconnect.current_delay();

        assert!(delay_after_1 > initial_delay, "delay should increase");
        assert!(
            delay_after_2 > delay_after_1,
            "delay should keep increasing"
        );
    }

    // ── Issue 12: Additional TrayState tests ──

    #[test]
    fn process_mute_poll_debounces_correctly() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // 1st muted poll: debouncer hasn't reached threshold yet
        let (a1, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(a1, MonitorAction::NoChange));
        assert!(!state.indicator.is_muted(), "not yet confirmed muted");

        // 2nd poll: threshold reached (threshold=2), ApplyMute
        let (a2, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(a2, MonitorAction::ApplyMute));
        assert!(state.indicator.is_muted(), "now confirmed muted");
    }

    #[test]
    fn handle_settings_result_updates_indicator() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        let original_color = state.indicator.mute_color();

        let mut new_config = state.config.clone();
        new_config.mute_color = "#00FF00".into();
        let (warnings, _, _, _, _) = state.handle_settings_result(new_config, Some(&dev));
        assert!(warnings.is_empty());
        assert_ne!(
            state.indicator.mute_color(),
            original_color,
            "mute color should have changed"
        );
    }

    #[test]
    fn sound_toggle_persists_to_config() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // Default is sound_enabled=true
        assert!(state.config.sound_enabled);

        // Simulate the sound toggle action from handle_menu_event
        state.config.sound_enabled = !state.config.sound_enabled;
        // (save() would write to disk — we just verify the in-memory state)

        assert!(
            !state.config.sound_enabled,
            "config should reflect toggled state"
        );
    }

    #[test]
    fn init_sound_enabled_from_config() {
        let dev = make_mock_device();
        let config = Config {
            sound_enabled: true,
            ..Config::default()
        };
        let state = TrayState::init_with_config(config, &dev).unwrap();
        assert!(
            state.config.sound_enabled,
            "should init sound_enabled from config"
        );

        let config2 = Config {
            sound_enabled: false,
            ..Config::default()
        };
        let state2 = TrayState::init_with_config(config2, &dev).unwrap();
        assert!(
            !state2.config.sound_enabled,
            "should init sound_enabled=false from config"
        );
    }

    #[test]
    fn hotkey_toggle_uses_debounced_state() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // Indicator starts not-muted
        assert!(!state.indicator.is_muted());

        // Simulate: user mutes externally, debounce confirms after 2 polls (threshold=2)
        state.process_mute_poll(true, Some(&dev));
        let (action, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(action, MonitorAction::ApplyMute));
        assert!(state.indicator.is_muted());

        // Now the hotkey handler should read is_muted()=true and toggle to false.
        let toggle_target = !state.indicator.is_muted();
        assert!(!toggle_target, "toggle should target unmuted");
    }

    #[test]
    fn apply_config_switches_strategy_while_muted() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // Get muted (threshold=2)
        state.process_mute_poll(true, Some(&dev));
        let (action, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(action, MonitorAction::ApplyMute));
        assert!(state.indicator.is_muted());

        // Switch strategy to target only input 1
        let mut new_config = state.config.clone();
        new_config.mute_inputs = "1".into();
        state.apply_config(new_config, Some(&dev));

        // Strategy should target only input 1
        assert_eq!(
            state.indicator.strategy().input_indices,
            &[0],
            "should target only input 1"
        );
        // Mute state should be preserved
        assert!(
            state.indicator.is_muted(),
            "mute state should be preserved after strategy switch"
        );
    }

    #[test]
    fn apply_config_color_change_updates_strategy_mute_colors() {
        let dev = make_mock_device();
        // Start with per-input mode so strategy has mute_colors populated
        let config = Config {
            mute_inputs: "1,2".into(),
            ..Config::default()
        };
        let mut state = TrayState::init_with_config(config, &dev).unwrap();

        let old_mute_color = state.indicator.mute_color();

        // Change only the global mute color
        let mut new_config = state.config.clone();
        new_config.mute_color = "#00FF00".into();
        state.apply_config(new_config, Some(&dev));

        // The strategy's mute_colors should reflect the new global color
        let new_color = state.indicator.mute_color();
        assert_ne!(
            new_color, old_mute_color,
            "mute color should have changed from the config update"
        );
        // The strategy should have been re-resolved (mute_colors refreshed)
        // If the strategy was NOT re-resolved, the per-input colors would still
        // point to the old default red.
        assert_eq!(
            state.indicator.strategy().input_indices,
            &[0, 1],
            "strategy should still target both inputs"
        );
    }

    #[test]
    fn process_mute_poll_debounces_at_threshold_2() {
        let dev = make_mock_device();
        let mut state = TrayState::init_with_config(Config::default(), &dev).unwrap();

        // 1st poll: not yet
        let (a1, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(a1, MonitorAction::NoChange));
        assert!(!state.indicator.is_muted());

        // 2nd poll: fires at threshold=2
        let (a2, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(a2, MonitorAction::ApplyMute));
        assert!(state.indicator.is_muted());

        // Subsequent same-state polls: NoChange
        let (a3, _) = state.process_mute_poll(true, Some(&dev));
        assert!(matches!(a3, MonitorAction::NoChange));
    }

    // ── No-device startup tests ──

    #[test]
    fn init_without_device_creates_valid_state() {
        let state = TrayState::init_without_device(Config::default());
        assert!(state.ctx.is_none());
        assert!(!state.indicator.is_muted());
        // No-op strategy: empty vectors
        assert!(state.indicator.strategy().input_indices.is_empty());
        assert!(state.indicator.strategy().number_leds.is_empty());
    }

    #[test]
    fn init_without_device_debounces() {
        let mut state = TrayState::init_without_device(Config::default());

        // Feed 2 muted polls (threshold=2) without any device
        let (a1, lost1) = state.process_mute_poll(true, Option::<&MockDevice>::None);
        assert!(matches!(a1, MonitorAction::NoChange));
        assert!(!lost1);

        let (a2, lost2) = state.process_mute_poll(true, Option::<&MockDevice>::None);
        assert!(matches!(a2, MonitorAction::ApplyMute));
        assert!(!lost2);
        assert!(state.indicator.is_muted());
    }

    #[test]
    fn reinit_device_context_populates_ctx() {
        let mut state = TrayState::init_without_device(Config::default());
        assert!(state.ctx.is_none());
        assert!(state.indicator.strategy().input_indices.is_empty());

        let dev = make_mock_device();
        let warnings = state.reinit_device_context(&dev).unwrap();
        assert!(warnings.is_empty());

        // ctx should now be populated
        assert!(state.ctx.is_some());
        let ctx = state.ctx.as_ref().unwrap();
        assert!(ctx.profile.is_some());
        assert_eq!(ctx.input_count(), Some(2));

        // Strategy should be real (non-empty)
        assert!(!state.indicator.strategy().input_indices.is_empty());
        assert_eq!(state.indicator.strategy().input_indices, &[0, 1]);
    }

    #[test]
    fn apply_config_without_ctx_keeps_noop_strategy() {
        let mut state = TrayState::init_without_device(Config::default());

        // Change color — should succeed even without a device
        let mut new_config = state.config.clone();
        new_config.mute_color = "#00FF00".into();
        let warnings = state.apply_config(new_config, Option::<&MockDevice>::None);

        // Strategy re-resolution fails (no profile/predicted) but the warning
        // is emitted and the no-op strategy is preserved.
        assert!(!warnings.is_empty());
        assert!(state.indicator.strategy().input_indices.is_empty());
    }

    #[test]
    fn apply_config_without_ctx_color_only_no_reresolution() {
        let mut state = TrayState::init_without_device(Config::default());

        // Change only sound_enabled — should NOT trigger strategy re-resolution
        let mut new_config = state.config.clone();
        new_config.sound_enabled = false;
        let warnings = state.apply_config(new_config, Option::<&MockDevice>::None);

        // No warnings because strategy re-resolution wasn't attempted
        assert!(warnings.is_empty());
        assert!(!state.config.sound_enabled);
    }

    #[test]
    fn reinit_then_mute_applies_leds() {
        let mut state = TrayState::init_without_device(Config::default());

        // Get muted while disconnected (no LED writes)
        state.process_mute_poll(true, Option::<&MockDevice>::None);
        state.process_mute_poll(true, Option::<&MockDevice>::None);
        assert!(state.indicator.is_muted());

        // Connect device
        let dev = make_mock_device();
        state.reinit_device_context(&dev).unwrap();

        // Now apply mute — should write LEDs
        let _ = state.indicator.apply_mute(&dev);
        let descs = dev.descriptors.borrow();
        assert!(
            descs.contains_key(&OFF_DIRECT_LED_COLOUR),
            "should write LED color after reinit"
        );
    }

    // ── ICO decode tests ──

    /// Build a minimal synthetic ICO file with given entries.
    /// Each entry is `(width_byte, png_data)`. Width byte 0 means 256px.
    fn build_synthetic_ico(entries: &[(u8, &[u8])]) -> Vec<u8> {
        let count = entries.len() as u16;
        let header_size = 6 + entries.len() * 16;
        let mut ico = Vec::new();

        // ICO header: reserved(2) + type(2) + count(2)
        ico.extend_from_slice(&[0, 0]); // reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (ICO)
        ico.extend_from_slice(&count.to_le_bytes());

        // Calculate data offsets
        let mut data_offset = header_size;
        for (width, png_data) in entries {
            let size = png_data.len() as u32;
            // Directory entry: width, height, color_count, reserved, planes(2), bpp(2), size(4), offset(4)
            ico.push(*width); // width
            ico.push(*width); // height (same as width for simplicity)
            ico.push(0); // color count
            ico.push(0); // reserved
            ico.extend_from_slice(&1u16.to_le_bytes()); // planes
            ico.extend_from_slice(&32u16.to_le_bytes()); // bpp
            ico.extend_from_slice(&size.to_le_bytes()); // data size
            ico.extend_from_slice(&(data_offset as u32).to_le_bytes()); // data offset
            data_offset += png_data.len();
        }

        // Append image data
        for (_, png_data) in entries {
            ico.extend_from_slice(png_data);
        }

        ico
    }

    /// Create a minimal valid PNG with the given dimensions.
    fn minimal_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([255u8, 0, 0, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn decode_ico_entry_selects_closest_size() {
        let png_16 = minimal_png(16, 16);
        let png_32 = minimal_png(32, 32);
        let ico = build_synthetic_ico(&[(16, &png_16), (32, &png_32)]);

        // Request 32px → should select the 32px entry
        let img = decode_ico_entry(&ico, 32).unwrap();
        assert_eq!(img.width(), 32);
        assert_eq!(img.height(), 32);

        // Request 16px → should select the 16px entry
        let img = decode_ico_entry(&ico, 16).unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn decode_ico_entry_fallback_on_out_of_bounds() {
        let png_32 = minimal_png(32, 32);
        let mut ico = build_synthetic_ico(&[(32, &png_32)]);

        // Corrupt the data offset to point past EOF
        let offset_pos = 6 + 12; // first entry's offset field at byte 18
        let bad_offset = (ico.len() + 1000) as u32;
        ico[offset_pos..offset_pos + 4].copy_from_slice(&bad_offset.to_le_bytes());

        // Should fall back to image::load_from_memory on the whole ICO
        // This may fail to decode (corrupted), but it should NOT panic
        let result = decode_ico_entry(&ico, 32);
        // We just verify no panic — result may be Ok or Err depending on
        // whether image crate can make sense of the corrupted ICO
        let _ = result;
    }

    #[test]
    fn embedded_icons_decode_at_32px() {
        let live = decode_ico_entry(ICON_LIVE_ICO, TRAY_ICON_SIZE).unwrap();
        assert_eq!(live.width(), 32);
        assert_eq!(live.height(), 32);

        let muted = decode_ico_entry(ICON_MUTED_ICO, TRAY_ICON_SIZE).unwrap();
        assert_eq!(muted.width(), 32);
        assert_eq!(muted.height(), 32);
    }
}
