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

use global_hotkey::GlobalHotKeyEvent;
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
    log::info!(
        "[focusmute] v{} starting (hotkey={}, inputs={}, color={}, sound={}, notifications={})",
        env!("CARGO_PKG_VERSION"),
        config.keyboard.hotkey,
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
    );
    if !config.hooks.on_mute_command.trim().is_empty()
        || !config.hooks.on_unmute_command.trim().is_empty()
    {
        log::info!(
            "[hooks] on_mute={:?}, on_unmute={:?}",
            config.hooks.on_mute_command,
            config.hooks.on_unmute_command,
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

    // Channel for background → main thread communication
    let (tx, rx): (mpsc::Sender<Msg>, Receiver<Msg>) = mpsc::channel();

    // Spawn background poll thread
    let bg_handle = if let Some(ref monitor) = main_monitor {
        Some(P::spawn_poll_thread(Arc::clone(monitor), tx))
    } else {
        log::warn!("[audio] no monitor available — mute polling disabled");
        None
    };

    // Register for USB device hotplug notifications (event-driven, no polling).
    P::register_device_notifications();

    // Main event loop
    let menu_rx = MenuEvent::receiver();
    let hotkey_rx = GlobalHotKeyEvent::receiver();
    let mut poll_thread_dead = false;

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
            if state.indicator.is_muted() {
                tray.set_icon(Some(state::icon::icon_muted())).ok();
                tray.set_tooltip(Some("FocusMute — Muted")).ok();
                tray_menu.status_item.set_text("Muted");
            } else {
                tray.set_icon(Some(state::icon::icon_live())).ok();
                tray.set_tooltip(Some("FocusMute — Live")).ok();
            }
        }

        // 3. Drain background messages (non-blocking)
        loop {
            match rx.try_recv() {
                Ok(Msg::MutePoll(muted)) => {
                    let (action, device_lost) = state.process_mute_poll(muted, device.as_ref());
                    if device_lost {
                        log::warn!("[device] disconnected (communication error)");
                        device = None;
                        tray_menu.set_device_connected(false, &tray);
                    }
                    state::apply_mute_ui(
                        action,
                        &tray,
                        &tray_menu,
                        &state,
                        &mut resources,
                        device.is_some(),
                    );
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
            if event.id == resources.hotkey.id
                && let Some(ref m) = main_monitor
                && let Err(e) = m.set_muted(!state.indicator.is_muted())
            {
                log::warn!("[mute] failed to toggle mute: {e}");
            }
        }

        // 6. Wait for events (platform-specific sleep/block)
        P::wait_for_events();
    }

    // Cleanup — join background thread, unmute, restore LEDs, then drop monitor.
    // Joining before drop ensures the monitor is dropped on the main thread
    // (important for COM cleanup on Windows).
    log::info!("[focusmute] shutting down");
    RUNNING.store(false, Ordering::SeqCst);
    if let Some(handle) = bg_handle {
        // Timed join: spawn a helper thread that joins the background thread,
        // and wait up to 3 seconds for it to finish. If the poll thread is stuck,
        // we proceed with shutdown rather than hanging indefinitely.
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = handle.join();
            let _ = done_tx.send(());
        });
        if done_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .is_err()
        {
            log::warn!(
                "[focusmute] poll thread did not exit within 3 s — proceeding with shutdown"
            );
        }
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
    if state.indicator.is_muted() {
        if let Some(ref dev) = device {
            state.restore_on_exit(dev);
        }
    }
    Ok(())
}
