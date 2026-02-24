//! Shared tray event loop — extracted from the ~80% identical code in
//! `windows.rs` and `linux.rs`. Platform-specific behavior is injected
//! via the [`PlatformAdapter`] trait.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;

use focusmute_lib::audio::MuteMonitor;
use focusmute_lib::config::Config;
use focusmute_lib::device::open_device_by_serial;

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

    /// Label for the autostart menu item, e.g. `"Start with Windows"`.
    fn autostart_label() -> &'static str;
}

/// Shared tray event loop.
///
/// Handles config load, device open, monitor creation, menu/icon build,
/// background thread spawn, and the main event loop.  Platform-specific
/// bits are injected via `P: PlatformAdapter`.
pub fn run_core<P: PlatformAdapter>() -> focusmute_lib::error::Result<()> {
    P::platform_init()?;

    // Open device and initialise shared state
    let config = Config::load();
    let initial_device = open_device_by_serial(&config.device_serial)?;
    let mut state = TrayState::init(&initial_device)?;

    // Create audio monitor on the main thread
    let main_monitor: Option<Arc<P::Monitor>> = P::create_monitor().map(Arc::new);

    // Check initial mute state
    let initial_muted = main_monitor.as_ref().is_some_and(|m| m.is_muted());

    let mut device = Some(initial_device);
    if initial_muted && let Some(ref dev) = device {
        state.set_initial_muted(true, dev);
    }

    // Init audio/hotkey resources
    let mut resources = TrayResources::init(&state.config)?;

    // Build tray menu and icon
    let (menu, tray_menu) =
        state::build_tray_menu(&state.config, initial_muted, P::autostart_label());
    let tray = state::build_tray_icon(initial_muted, menu)?;

    // Channel for background → main thread communication
    let (tx, rx): (mpsc::Sender<Msg>, Receiver<Msg>) = mpsc::channel();

    // Spawn background poll thread
    let bg_handle = if let Some(ref monitor) = main_monitor {
        Some(P::spawn_poll_thread(Arc::clone(monitor), tx))
    } else {
        log::warn!("No audio monitor available — mute polling disabled");
        None
    };

    // Main event loop
    let menu_rx = MenuEvent::receiver();
    let hotkey_rx = GlobalHotKeyEvent::receiver();

    loop {
        if !RUNNING.load(Ordering::SeqCst) {
            break;
        }

        // 1. Platform event pump
        P::pump_events();

        // 2. Reconnect
        if device.is_none()
            && let Some(new_dev) = state.try_reconnect()
        {
            device = Some(new_dev);
            tray_menu.set_device_connected(true);
        }

        // 3. Drain mute polls (non-blocking)
        while let Ok(Msg::MutePoll(muted)) = rx.try_recv() {
            let (action, device_lost) = state.process_mute_poll(muted, device.as_ref());
            if device_lost {
                device = None;
                tray_menu.set_device_connected(false);
            }
            state::apply_mute_ui(action, &tray, &tray_menu, &state, &resources);
        }

        // 4. Menu events
        while let Ok(event) = menu_rx.try_recv() {
            let toggle_mute = |is_muted: bool| {
                if let Some(ref m) = main_monitor {
                    let _ = m.set_muted(!is_muted);
                }
            };
            let quit = state::handle_menu_event(
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
        }

        // 5. Hotkey events
        while let Ok(event) = hotkey_rx.try_recv() {
            if event.id == resources.hotkey.id
                && let Some(ref m) = main_monitor
            {
                let _ = m.set_muted(!state.indicator.is_muted());
            }
        }

        // 6. Wait for events (platform-specific sleep/block)
        P::wait_for_events();
    }

    // Cleanup — join background thread, then restore LED state.
    // Joining before drop ensures the monitor is dropped on the main thread
    // (important for COM cleanup on Windows).
    RUNNING.store(false, Ordering::SeqCst);
    if let Some(handle) = bg_handle {
        let _ = handle.join();
    }
    drop(main_monitor);

    if let Some(ref dev) = device {
        state.restore_on_exit(dev);
    }
    Ok(())
}
