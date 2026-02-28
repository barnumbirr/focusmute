//! Linux system tray — GTK event loop, PulseAudio monitoring.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use focusmute_lib::audio::{MuteMonitor, PulseAudioMonitor};

use super::shared::{self, PlatformAdapter};
use super::state::Msg;
use crate::RUNNING;

/// Detect if running under Wayland (global hotkeys may not work).
fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

pub struct LinuxAdapter;

impl PlatformAdapter for LinuxAdapter {
    type Monitor = PulseAudioMonitor;

    fn platform_init() -> focusmute_lib::error::Result<()> {
        gtk::init().expect("Failed to initialize GTK");
        // Periodic wakeup so `gtk::main_iteration_do(true)` returns at least
        // every 50ms, keeping the event loop responsive to non-GTK events
        // (mute polls, menu events, hotkeys).
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), || {
            gtk::glib::ControlFlow::Continue
        });
        if is_wayland() {
            log::warn!(
                "Global hotkey may not work on Wayland. \
                 Use the tray menu to toggle mute."
            );
        }
        Ok(())
    }

    fn create_monitor() -> Option<PulseAudioMonitor> {
        PulseAudioMonitor::new().ok()
    }

    fn spawn_poll_thread(monitor: Arc<PulseAudioMonitor>, tx: mpsc::Sender<Msg>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            // Allow PulseAudio to settle before starting the poll loop.
            // Without this, the first few readings may be stale.
            focusmute_lib::audio::stabilize_pulseaudio(&monitor);

            while RUNNING.load(Ordering::SeqCst) {
                monitor.wait_for_change(Duration::from_millis(250));
                monitor.refresh();
                let muted = monitor.is_muted();
                if tx.send(Msg::MutePoll(muted)).is_err() {
                    break;
                }
            }
        })
    }

    fn pump_events() {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    fn wait_for_events() {
        // Block until the next GTK event. The 50ms timer registered in
        // platform_init() guarantees we wake up frequently enough to
        // process non-GTK events (mute polls, menu, hotkeys).
        gtk::main_iteration_do(true);
    }
}

pub fn run() -> focusmute_lib::error::Result<()> {
    shared::run_core::<LinuxAdapter>()
}
