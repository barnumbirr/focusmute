//! Windows system tray — Win32 message loop, WASAPI monitoring.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use focusmute_lib::audio::{self, MuteMonitor, WasapiMonitor};

use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    TranslateMessage, WM_QUIT,
};

use super::shared::{self, PlatformAdapter};
use super::state::Msg;
use crate::RUNNING;

/// Pump all pending Win32 messages. Required for tray-icon and global-hotkey
/// to receive their internal window messages on Windows.
fn pump_messages() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                RUNNING.store(false, Ordering::SeqCst);
                return;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub struct WindowsAdapter;

impl PlatformAdapter for WindowsAdapter {
    type Monitor = WasapiMonitor;

    fn platform_init() -> focusmute_lib::error::Result<()> {
        audio::com_init()?;
        Ok(())
    }

    fn create_monitor() -> Option<WasapiMonitor> {
        WasapiMonitor::new().ok()
    }

    fn spawn_poll_thread(monitor: Arc<WasapiMonitor>, tx: mpsc::Sender<Msg>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            if let Err(e) = audio::com_init() {
                log::error!("Audio init error: {e}");
                return;
            }

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
        pump_messages();
    }

    fn wait_for_events() {
        unsafe {
            MsgWaitForMultipleObjects(None, false, 50, QS_ALLINPUT);
        }
    }
}

pub fn run() -> focusmute_lib::error::Result<()> {
    shared::run_core::<WindowsAdapter>()
}
