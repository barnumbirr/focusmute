//! Shared tray event loop — extracted from the ~80% identical code in
//! `windows.rs` and `linux.rs`. Platform-specific behavior is injected
//! via the [`PlatformAdapter`] trait.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use focusmute_lib::audio::MuteMonitor;
use focusmute_lib::config::Config;
use focusmute_lib::device::{ScarlettDevice, open_device_by_serial};
use focusmute_lib::monitor::MonitorAction;

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use muda::MenuEvent;

use super::state::{self, Msg, TrayResources, TrayState};
use crate::RUNNING;

/// Platform-specific hooks that differ between Windows and Linux.
///
/// Each platform implements this trait once; `run_core` provides the
/// shared event loop, config load, device open, menu build, etc.
pub trait PlatformAdapter {
    type Monitor: MuteMonitor + Send + Sync + 'static;

    /// One-time platform init (GTK, COM, etc.). Called before anything else.
    fn platform_init() -> focusmute_lib::error::Result<()>;

    /// Create the audio mute monitor on the **main thread**.
    /// Returns `None` if the audio subsystem is unavailable.
    fn create_monitor() -> Option<Self::Monitor>;

    /// Spawn the background polling thread.
    /// The thread should call `monitor.wait_for_change()` / `refresh()` / `is_muted()`
    /// and send `Msg::MutePoll` over `tx`.
    fn spawn_poll_thread(monitor: Arc<Self::Monitor>, tx: mpsc::Sender<Msg>) -> JoinHandle<()>;

    /// Pump platform-specific events (Win32 messages, GTK iterations).
    fn pump_events();

    /// Block until the next platform event or a reasonable timeout.
    fn wait_for_events();

    /// Register for device hotplug notifications (called once at startup).
    fn register_device_notifications() {}

    /// Check whether a device-removed event has fired since the last call.
    /// Resets the flag so subsequent calls return false until the next event.
    fn check_device_removed() -> bool {
        false
    }
}

/// Shared tray event loop.
///
/// Handles config load, device open, monitor creation, menu/icon build,
/// background thread spawn, and the main event loop.  Platform-specific
/// bits are injected via `P: PlatformAdapter`.
pub fn run_core<P: PlatformAdapter>() -> focusmute_lib::error::Result<()> {
    P::platform_init()?;

    // Open device and initialise shared state.
    // If the device isn't connected yet, start with a no-op strategy and
    // let the reconnect loop pick it up later.
    let first_run = Config::is_first_run();
    let (config, parse_warnings) = Config::load_with_warnings();
    for w in &parse_warnings {
        log::warn!("[config] {w}");
    }
    if let Some(config_path) = Config::path() {
        log::info!("[config] {}", config_path.display());
    }
    let sync_banner = if config.system.browser_sync_port > 0 {
        config.system.browser_sync_port.to_string()
    } else {
        "off".to_string()
    };
    log::info!(
        "[focusmute] v{} starting (hotkey={}, ptt={}, inputs={}, color={}, sound={}, notifications={}, log={}, sync={}, reverse={}, blink={})",
        env!("CARGO_PKG_VERSION"),
        config.keyboard.hotkey,
        if config.keyboard.push_to_talk_hotkey.is_empty() {
            "off"
        } else {
            &config.keyboard.push_to_talk_hotkey
        },
        config.indicator.mute_inputs,
        config.indicator.mute_color,
        if config.sound.sound_enabled {
            "on"
        } else {
            "off"
        },
        if config.system.notifications_enabled {
            "on"
        } else {
            "off"
        },
        config.system.log_level,
        sync_banner,
        if config.system.browser_sync_reverse {
            "on"
        } else {
            "off"
        },
        if config.indicator.blink_on_talk {
            "on"
        } else {
            "off"
        },
    );
    if !config.hooks.on_mute_url.trim().is_empty() || !config.hooks.on_unmute_url.trim().is_empty()
    {
        log::info!(
            "[hooks] on_mute={:?}, on_unmute={:?}",
            config.hooks.on_mute_url,
            config.hooks.on_unmute_url,
        );
    }
    let (mut state, mut device) = match open_device_by_serial(&config.system.device_serial) {
        Ok(dev) => {
            let info = dev.info();
            log::info!(
                "[device] {} (firmware {}{})",
                info.device_name,
                info.firmware,
                info.serial
                    .as_ref()
                    .map(|s| format!(", serial {s}"))
                    .unwrap_or_default(),
            );
            let st = TrayState::init_with_config(config, &dev)?;
            (st, Some(dev))
        }
        Err(e) => {
            log::warn!("[device] not found at startup ({e}) — starting without device");
            (TrayState::init_without_device(config), None)
        }
    };

    // Create audio monitor on the main thread
    let main_monitor: Option<Arc<P::Monitor>> = P::create_monitor().map(Arc::new);

    // Check initial mute state
    let initial_muted = main_monitor.as_ref().is_some_and(|m| m.is_muted());
    log::info!(
        "[mute] initial state: {}",
        if initial_muted { "muted" } else { "live" }
    );

    if initial_muted && let Some(ref dev) = device {
        state.set_initial_muted(true, dev);
    }

    // Init audio/hotkey resources
    let (mut resources, sound_warnings) = TrayResources::init(&state.config)?;

    // Build tray menu and icon
    let device_connected = device.is_some();
    let (menu, tray_menu) = state::build_tray_menu(&state.config, initial_muted);
    let tray = state::build_tray_icon(initial_muted, device_connected, menu)?;

    // If no device at startup, show disconnected status immediately.
    // When connected, only update the reconnect label — the status text
    // already reflects the initial mute state from build_tray_menu.
    if !device_connected {
        tray_menu.set_device_connected(false, &tray);
    } else {
        tray_menu.set_reconnect_label(true);
    }

    // Show startup warnings (parse errors + validation errors + sound errors)
    {
        const MAX_SOUND_BYTES: u64 = 10_000_000;
        let mut all_warnings = parse_warnings;
        all_warnings.extend(sound_warnings);
        let input_count = state.ctx.as_ref().and_then(|c| c.input_count());
        if let Err(errs) = state.config.validate(input_count, MAX_SOUND_BYTES) {
            for e in &errs {
                let msg = e.to_string();
                log::warn!("[config] {msg}");
                all_warnings.push(msg);
            }
        }
        if !all_warnings.is_empty() {
            state::show_startup_warnings(&all_warnings);
        }
    }

    // First-run welcome notification
    if first_run {
        let hotkey = &state.config.keyboard.hotkey;
        crate::notification::Notifier::show_oneshot(&format!(
            "FocusMute running. Hotkey: {hotkey}. Right-click tray icon for settings."
        ));
    }

    // Channel for background → main thread communication.
    // Each background thread gets its own clone; the main thread drops its
    // sender so that TryRecvError::Disconnected fires when all threads die.
    let (tx, rx): (mpsc::Sender<Msg>, Receiver<Msg>) = mpsc::channel();

    // Spawn background poll thread
    let bg_handle = if let Some(ref monitor) = main_monitor {
        Some(P::spawn_poll_thread(Arc::clone(monitor), tx.clone()))
    } else {
        log::warn!("[audio] no monitor available — mute polling disabled");
        None
    };

    // Spawn browser sync listener for extension mute sync. The reverse slot
    // carries user-initiated transitions to the extension's poll requests;
    // it always exists so the reverse-sync toggle hot-applies.
    let reverse_slot = Arc::new(super::browser_sync::ReverseSlot::new());
    let sync_port = state.config.system.browser_sync_port;
    let sync_handle = if sync_port > 0 {
        log::info!("[sync] starting on port {sync_port}");
        Some(super::browser_sync::spawn_sync_thread(
            sync_port,
            tx.clone(),
            Arc::clone(&reverse_slot),
        ))
    } else {
        log::info!("[sync] disabled (port=0)");
        None
    };

    // Main thread no longer holds a sender — channel disconnects when
    // all background threads exit.
    drop(tx);

    // Register for USB device hotplug notifications (event-driven, no polling).
    P::register_device_notifications();

    // Main event loop
    let menu_rx = MenuEvent::receiver();
    let hotkey_rx = GlobalHotKeyEvent::receiver();
    let mut poll_thread_dead = false;
    let mut ptt_active = false;
    let mut ptt_noop_logged = false;
    let mut browser_sync_pending = false;
    let mut blink = super::blink::BlinkState::new();

    loop {
        if !RUNNING.load(Ordering::SeqCst) {
            break;
        }

        // 1. Platform event pump — dispatches WM_DEVICECHANGE (among others)
        //    which sets the device-removed flag checked below.
        P::pump_events();

        // 1b. Device removal detection (event-driven via RegisterDeviceNotification).
        if device.is_some() && P::check_device_removed() {
            log::warn!("[device] removed (USB hotplug event)");
            device = None;
            tray_menu.set_device_connected(false, &tray);
        }

        // 2. Reconnect
        if device.is_none()
            && let Some(new_dev) = state.try_reconnect()
        {
            log::info!("[device] reconnected: {}", new_dev.info().device_name);
            device = Some(new_dev);
            tray_menu.set_device_connected(true, &tray);
            // Restore correct icon based on current mute state
            state::set_tray_mute_state(&tray, state.indicator.is_muted());
            if state.indicator.is_muted() {
                tray_menu.status_item.set_text("Muted");
            }
        }

        // 3. Drain background messages (non-blocking)
        let mut browser_mute_target: Option<bool> = None;
        loop {
            match rx.try_recv() {
                Ok(Msg::MutePoll(muted)) => {
                    let (action, device_lost) = state.process_mute_poll(muted, device.as_ref());
                    if device_lost {
                        log::warn!("[device] disconnected (communication error)");
                        device = None;
                        tray_menu.set_device_connected(false, &tray);
                    }
                    // Captured before the clear below: a pending flag means
                    // this transition originated in the browser.
                    let browser_originated = browser_sync_pending;
                    let suppress_sound =
                        browser_sync_pending && state.config.sound.suppress_browser_sync_sound;
                    // Only clear the flag when a real state change fires —
                    // NoChange means the debouncer hasn't confirmed yet.
                    if !matches!(action, MonitorAction::NoChange) {
                        browser_sync_pending = false;
                    }
                    // Reverse sync: mirror USER-initiated transitions to the
                    // extension. Browser-originated ones are echoes — the
                    // meeting page already shows that state.
                    if state.config.system.browser_sync_reverse && !browser_originated {
                        match action {
                            MonitorAction::ApplyMute => {
                                log::debug!("[sync] reverse action queued: mute");
                                reverse_slot.set(true);
                            }
                            MonitorAction::ClearMute => {
                                log::debug!("[sync] reverse action queued: unmute");
                                reverse_slot.set(false);
                            }
                            _ => {}
                        }
                    }
                    state::apply_mute_ui(
                        action,
                        &tray,
                        &tray_menu,
                        &state,
                        &mut resources,
                        device.is_some(),
                        suppress_sound,
                    );
                }
                Ok(Msg::BrowserMute(muted)) => {
                    // Coalesce: Meet flickers its mute state during join-screen
                    // render, producing bursts like false→true→false within one
                    // drain pass. is_muted() is callback-cached and lags set_muted,
                    // so acting per-message evaluates every message against the
                    // same stale snapshot and can latch a transient state. Browser
                    // sync is level-triggered — only the last state matters.
                    browser_mute_target = Some(muted);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if !poll_thread_dead {
                        log::error!("[audio] monitor thread stopped unexpectedly");
                        poll_thread_dead = true;
                    }
                    break;
                }
            }
        }
        if let Some(muted) = browser_mute_target {
            handle_browser_mute(
                main_monitor.as_deref(),
                muted,
                state.indicator.is_muted(),
                &mut browser_sync_pending,
            );
        }

        // 3c. Muted-talk blink — rides the ~50 ms loop wake; reads meters at
        // most every METER_INTERVAL while muted with the feature enabled.
        if state.config.indicator.blink_on_talk
            && state.indicator.is_muted()
            && let Some(ref dev) = device
        {
            let now = std::time::Instant::now();
            let talking = if blink.meter_due(now) {
                blink.note_meter_read(now);
                let read_result =
                    focusmute_lib::meter::read_meters(dev, focusmute_lib::meter::METER_COUNT);
                let elapsed = now.elapsed();
                // A meter read blocks this loop, and the loop applies mute
                // transitions — slow reads directly delay unmute. Surface it.
                if elapsed > std::time::Duration::from_millis(50) {
                    log::warn!("[blink] slow meter read: {} ms", elapsed.as_millis());
                }
                match read_result {
                    Ok(levels) => {
                        let level = focusmute_lib::meter::max_input_level(
                            &levels,
                            &state.indicator.strategy().input_indices,
                        );
                        log::debug!(
                            "[blink] meter read {} ms, level {} (threshold {})",
                            elapsed.as_millis(),
                            level,
                            state.config.indicator.talk_threshold
                        );
                        Some(level >= state.config.indicator.talk_threshold)
                    }
                    Err(e) => {
                        log::warn!(
                            "[blink] meter read failed after {} ms: {e}",
                            elapsed.as_millis()
                        );
                        None
                    }
                }
            } else {
                None
            };
            if let Some(action) = blink.advance(now, talking) {
                apply_blink_action(dev, &state, action);
            }
        } else if let Some(action) = blink.reset(state.indicator.is_muted())
            && let Some(ref dev) = device
        {
            apply_blink_action(dev, &state, action);
        }

        // 4. Menu events
        while let Ok(event) = menu_rx.try_recv() {
            let toggle_mute = |is_muted: bool| {
                if let Some(ref m) = main_monitor
                    && let Err(e) = m.set_muted(!is_muted)
                {
                    log::warn!("[mute] failed to toggle mute: {e}");
                }
            };
            let (quit, force_reconnect) = state::handle_menu_event(
                &event,
                &tray_menu,
                &mut state,
                &mut device,
                &mut resources,
                &toggle_mute,
            );
            if quit {
                RUNNING.store(false, Ordering::SeqCst);
                break;
            }
            if force_reconnect {
                device = None;
                tray_menu.set_device_connected(false, &tray);
            }
        }

        // 5. Hotkey events
        while let Ok(event) = hotkey_rx.try_recv() {
            let Some(ref m) = main_monitor else {
                continue;
            };
            if event.id == resources.hotkey.toggle_id {
                // Toggle hotkey: act on press only
                if event.state != HotKeyState::Pressed {
                    continue;
                }
                ptt_active = false;
                ptt_noop_logged = false;
                let target_muted = !state.indicator.is_muted();
                log::info!(
                    "[hotkey] toggle → {}",
                    if target_muted { "muting" } else { "unmuting" }
                );
                if let Err(e) = m.set_muted(target_muted) {
                    log::warn!("[hotkey] failed to set mute state: {e}");
                }
            } else if resources.hotkey.ptt_id.is_some_and(|id| event.id == id) {
                // PTT hotkey: press = unmute (if muted), release = re-mute (if PTT activated)
                match event.state {
                    HotKeyState::Pressed => {
                        if state.indicator.is_muted() {
                            ptt_active = true;
                            ptt_noop_logged = false;
                            log::info!("[ptt] held → unmuting");
                            if let Err(e) = m.set_muted(false) {
                                log::warn!("[ptt] failed to unmute: {e}");
                            }
                        } else if !ptt_noop_logged {
                            ptt_noop_logged = true;
                            log::debug!("[ptt] already unmuted, ignoring press");
                        }
                    }
                    HotKeyState::Released => {
                        if ptt_active {
                            ptt_active = false;
                            log::info!("[ptt] released → re-muting");
                            if let Err(e) = m.set_muted(true) {
                                log::warn!("[ptt] failed to re-mute: {e}");
                            }
                        }
                    }
                }
            }
        }

        // 6. Wait for events (platform-specific sleep/block)
        P::wait_for_events();
    }

    // Cleanup — join background threads, unmute, restore LEDs, then drop monitor.
    // Joining before drop ensures the monitor is dropped on the main thread
    // (important for COM cleanup on Windows).
    log::info!("[focusmute] shutting down");
    RUNNING.store(false, Ordering::SeqCst);

    // Timed join helper — waits up to `timeout` for a thread to finish.
    let timed_join = |handle: JoinHandle<()>, name: &str| {
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });
        if done_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .is_err()
        {
            log::warn!("[focusmute] {name} thread did not exit within 3 s — proceeding");
        }
    };

    if let Some(handle) = bg_handle {
        timed_join(handle, "poll");
    }
    if let Some(handle) = sync_handle {
        timed_join(handle, "sync");
    }

    // Unmute all inputs so the user isn't left silently muted after exit
    // (LEDs return to normal state and can no longer indicate mute).
    if let Some(ref monitor) = main_monitor
        && monitor.is_muted()
    {
        match monitor.set_muted(false) {
            Ok(()) => log::info!("[mute] unmuted on exit"),
            Err(e) => log::warn!("[mute] failed to unmute on exit: {e}"),
        }
    }
    drop(main_monitor);

    // Only restore LEDs if we were muted (i.e. we actually changed them).
    // Skipping when live avoids a spurious IOCTL that can fail during shutdown.
    if state.indicator.is_muted()
        && let Some(ref dev) = device
    {
        state.restore_on_exit(dev);
    }
    Ok(())
}

/// Apply a (coalesced) browser-sync target, then reconcile the suppression flag.
///
/// `committed_muted` is the debouncer's confirmed (displayed) mute state — the
/// synchronous source of truth for whether a UI transition will fire, unlike
/// Perform a blink LED action: the off-phase blanks the indicator's number
/// LEDs, the solid phase repaints the regular mute indicator. Failures are
/// debug-logged only — a missed blink frame is cosmetic, and device loss is
/// detected by the regular poll path.
fn apply_blink_action(
    device: &impl ScarlettDevice,
    state: &TrayState,
    action: super::blink::LedAction,
) {
    let result = match action {
        super::blink::LedAction::Solid => state.indicator.apply_mute(device),
        super::blink::LedAction::Off => state
            .indicator
            .strategy()
            .number_leds
            .iter()
            .try_for_each(|&led| focusmute_lib::led::set_single_led(device, led, 0)),
    };
    if let Err(e) = result {
        log::debug!("[blink] LED update failed: {e}");
    }
}

/// the monitor's callback-cached `is_muted()` which lags `set_muted`.
///
/// `browser_sync_pending` is armed to suppress the beep of a browser-caused UI
/// transition, and is normally cleared by that transition's confirming
/// `MutePoll` in the main loop. But a *net-zero* browser flicker — Meet's join
/// screen blips mute→unmute across drain passes without the debouncer ever
/// confirming — produces no confirming transition, so the flag would be
/// stranded `true` and silence the next, unrelated, user-initiated mute
/// (possibly minutes later). If the browser's net target already equals the
/// displayed state, there is nothing left to confirm: clear the flag now.
fn handle_browser_mute<M: MuteMonitor>(
    monitor: Option<&M>,
    target: bool,
    committed_muted: bool,
    browser_sync_pending: &mut bool,
) {
    apply_browser_mute(monitor, target, browser_sync_pending);
    if target == committed_muted {
        *browser_sync_pending = false;
    }
}

/// Apply a (coalesced) browser-sync mute state to the audio monitor.
///
/// Called at most once per drain pass with the last `Msg::BrowserMute`
/// state received. The `is_muted()` gate keeps idempotent messages from
/// arming `browser_sync_pending` (which would suppress the beep of the
/// next user-initiated mute); on `set_muted` failure the flag is rolled
/// back for the same reason.
fn apply_browser_mute<M: MuteMonitor>(
    monitor: Option<&M>,
    muted: bool,
    browser_sync_pending: &mut bool,
) {
    let Some(m) = monitor else {
        log::warn!("[sync] browser mute ignored — no audio monitor");
        return;
    };
    if m.is_muted() == muted {
        log::debug!(
            "[sync] browser mute → already {} (no action)",
            if muted { "muted" } else { "unmuted" }
        );
    } else {
        log::info!(
            "[sync] browser mute → {}",
            if muted { "muting" } else { "unmuting" }
        );
        *browser_sync_pending = true;
        if let Err(e) = m.set_muted(muted) {
            log::warn!("[sync] failed to set mute state: {e}");
            *browser_sync_pending = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use focusmute_lib::audio::AudioError;
    use std::sync::Mutex;
    use std::time::Duration;

    /// Monitor that models the WASAPI backend's callback-cached state:
    /// `is_muted()` keeps returning the construction-time snapshot no matter
    /// what `set_muted` was called with (the real cache only updates when the
    /// COM change callback fires, long after a drain pass has finished).
    /// Records every `set_muted` call for assertion.
    struct LaggyMonitor {
        stale_muted: bool,
        fail_set: bool,
        set_calls: Mutex<Vec<bool>>,
    }

    impl LaggyMonitor {
        fn new(stale_muted: bool) -> Self {
            Self {
                stale_muted,
                fail_set: false,
                set_calls: Mutex::new(Vec::new()),
            }
        }

        fn failing(stale_muted: bool) -> Self {
            Self {
                fail_set: true,
                ..Self::new(stale_muted)
            }
        }

        fn calls(&self) -> Vec<bool> {
            self.set_calls.lock().unwrap().clone()
        }
    }

    impl MuteMonitor for LaggyMonitor {
        fn is_muted(&self) -> bool {
            self.stale_muted
        }

        fn set_muted(&self, muted: bool) -> focusmute_lib::audio::Result<()> {
            self.set_calls.lock().unwrap().push(muted);
            if self.fail_set {
                Err(AudioError::OperationFailed("test failure".into()))
            } else {
                Ok(())
            }
        }

        fn wait_for_change(&self, _timeout: Duration) -> bool {
            false
        }
    }

    /// Replay a burst through the drain-loop coalescing (last state wins)
    /// and the post-drain apply, exactly as `run_core` does.
    fn process_burst(monitor: &LaggyMonitor, burst: &[bool], pending: &mut bool) {
        let mut target: Option<bool> = None;
        for &muted in burst {
            target = Some(muted);
        }
        if let Some(muted) = target {
            apply_browser_mute(Some(monitor), muted, pending);
        }
    }

    // ── Burst coalescing (regression for the Meet join-screen flicker) ──

    /// The logged incident: muted user joins, Meet flickers false→true→false.
    /// Exactly one unmute must fire — the transient "muted" must not act.
    #[test]
    fn flicker_burst_while_muted_unmutes_once() {
        let monitor = LaggyMonitor::new(true);
        let mut pending = false;
        process_burst(&monitor, &[false, true, false], &mut pending);
        assert_eq!(monitor.calls(), vec![false]);
        assert!(pending);
    }

    /// The latch-up case the pre-coalescing code got wrong: user already
    /// unmuted, same flicker burst. Per-message handling muted on the
    /// transient "true" and then dropped the correcting "false" because the
    /// stale cache still read unmuted — leaving the interface muted while
    /// Meet showed unmuted. Coalescing must produce no action at all.
    #[test]
    fn flicker_burst_while_unmuted_is_noop() {
        let monitor = LaggyMonitor::new(false);
        let mut pending = false;
        process_burst(&monitor, &[false, true, false], &mut pending);
        assert!(monitor.calls().is_empty());
        assert!(!pending);
    }

    /// Burst ending in a real state change: only the final state acts.
    #[test]
    fn flicker_burst_ending_muted_mutes_once() {
        let monitor = LaggyMonitor::new(false);
        let mut pending = false;
        process_burst(&monitor, &[true, false, true], &mut pending);
        assert_eq!(monitor.calls(), vec![true]);
        assert!(pending);
    }

    #[test]
    fn empty_drain_does_nothing() {
        let monitor = LaggyMonitor::new(true);
        let mut pending = false;
        process_burst(&monitor, &[], &mut pending);
        assert!(monitor.calls().is_empty());
        assert!(!pending);
    }

    // ── Single-message gate semantics (preserved from the v0.9.1 beep fix) ──

    #[test]
    fn idempotent_message_does_not_arm_suppression_flag() {
        let monitor = LaggyMonitor::new(true);
        let mut pending = false;
        apply_browser_mute(Some(&monitor), true, &mut pending);
        assert!(monitor.calls().is_empty());
        assert!(!pending);
    }

    #[test]
    fn state_change_arms_suppression_flag_and_sets_mute() {
        let monitor = LaggyMonitor::new(false);
        let mut pending = false;
        apply_browser_mute(Some(&monitor), true, &mut pending);
        assert_eq!(monitor.calls(), vec![true]);
        assert!(pending);
    }

    #[test]
    fn set_muted_failure_rolls_back_suppression_flag() {
        let monitor = LaggyMonitor::failing(true);
        let mut pending = false;
        apply_browser_mute(Some(&monitor), false, &mut pending);
        assert_eq!(monitor.calls(), vec![false]);
        assert!(!pending);
    }

    #[test]
    fn missing_monitor_is_noop() {
        let mut pending = false;
        apply_browser_mute::<LaggyMonitor>(None, true, &mut pending);
        assert!(!pending);
    }

    // ── Net-zero flicker reconciliation ──
    //
    // `handle_browser_mute` clears `browser_sync_pending` when the browser's
    // net target already equals the committed (displayed) state, because no
    // confirming `MutePoll` transition will arrive to clear it otherwise.

    /// Replay a sequence of per-drain-pass browser targets through the full
    /// post-drain handling (apply + reconciliation) against a fixed committed
    /// UI state — one call per drain pass, as the main loop does.
    fn drive_drain_passes(
        monitor: &LaggyMonitor,
        committed_muted: bool,
        per_pass_targets: &[bool],
        pending: &mut bool,
    ) {
        for &target in per_pass_targets {
            handle_browser_mute(Some(monitor), target, committed_muted, pending);
        }
    }

    /// The incident this guards against: an unmuted user's Meet join flickers
    /// mute→unmute across two drain passes. The debouncer never confirms the
    /// blip, so the committed state stays unmuted and no `MutePoll` clears the
    /// flag. The flag must not survive the burst, or the next user mute is
    /// silent. Cache lags at unmuted throughout (worst case).
    #[test]
    fn net_zero_flicker_does_not_strand_pending() {
        let monitor = LaggyMonitor::new(false);
        let mut pending = false;
        drive_drain_passes(&monitor, false, &[true, false], &mut pending);
        assert!(
            !pending,
            "net-zero browser flicker stranded the suppression flag"
        );
    }

    /// A genuine browser mute (committed UI still unmuted, a real transition
    /// pending) must KEEP the flag armed so the confirming `MutePoll`'s beep
    /// is suppressed — reconciliation must not clear it prematurely.
    #[test]
    fn genuine_browser_mute_keeps_pending_until_confirmed() {
        let monitor = LaggyMonitor::new(false);
        let mut pending = false;
        handle_browser_mute(Some(&monitor), true, false, &mut pending);
        assert!(
            pending,
            "genuine browser transition must keep the suppression flag armed"
        );
    }

    /// Once the displayed state catches up to the browser target (transition
    /// confirmed), a repeat report at the now-matching state self-heals the
    /// flag rather than leaving it armed for a later unrelated mute.
    #[test]
    fn target_matching_committed_clears_pending() {
        let monitor = LaggyMonitor::new(true);
        let mut pending = true;
        handle_browser_mute(Some(&monitor), true, true, &mut pending);
        assert!(!pending);
    }
}
