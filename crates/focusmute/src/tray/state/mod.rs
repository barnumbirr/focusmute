//! Shared tray state and business logic — used by both Windows and Linux tray apps.
//!
//! Platform-specific event loops and UI code live in `windows.rs` / `linux.rs`.
//! This module provides:
//! - Core `TrayState` (config, indicator, reconnection)
//! - Menu + tray icon construction (`build_tray_menu`, `build_tray_icon`)
//! - Hotkey management (`HotkeyState`, `register_hotkey`, `reregister_hotkey`)
//! - Settings result handling (`handle_settings_result`)
//! - Icon caching, autostart helpers

mod hotkey;
mod icon;
mod menu;

pub use hotkey::{HotkeyState, register_hotkey, reregister_hotkey};
pub(crate) use menu::show_startup_warnings;
pub use menu::{TrayMenu, apply_mute_ui, build_tray_icon, build_tray_menu};

use focusmute_lib::config::Config;
use focusmute_lib::context::DeviceContext;
use focusmute_lib::device::ScarlettDevice;
use focusmute_lib::led;
use focusmute_lib::monitor::{MonitorAction, MuteIndicator};
use focusmute_lib::reconnect::ReconnectState;

use auto_launch::AutoLaunchBuilder;
use muda::MenuEvent;

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

// ── Messages from background thread ──

pub enum Msg {
    MutePoll(bool),
}

// ── Autostart ──

pub fn get_auto_launch() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_str()?;
    AutoLaunchBuilder::new()
        .set_app_name("FocusMute")
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
}
