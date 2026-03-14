//! Windows system tray — Win32 message loop, WASAPI monitoring,
//! USB device hotplug detection via `RegisterDeviceNotificationW`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use focusmute_lib::audio::{self, MuteMonitor, WasapiMonitor};

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

use super::shared::{self, PlatformAdapter};
use super::state::Msg;
use crate::RUNNING;

// ── Device hotplug notification ──

/// Set by the hidden window's WM_DEVICECHANGE handler when a Focusrite
/// device interface is removed.  Checked (and cleared) by the main loop.
static DEVICE_REMOVED: AtomicBool = AtomicBool::new(false);

// WM_DEVICECHANGE constants (not always exposed by the windows crate).
const WM_DEVICECHANGE: u32 = 0x0219;
const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;

/// Minimal `DEV_BROADCAST_DEVICEINTERFACE_W` — only the fixed-size header
/// fields we need for `RegisterDeviceNotificationW`.
#[repr(C)]
struct DevBroadcastDeviceInterface {
    dbcc_size: u32,
    dbcc_devicetype: u32,
    dbcc_reserved: u32,
    dbcc_classguid: windows::core::GUID,
    dbcc_name: [u16; 1],
}

/// Window procedure for the hidden hotplug-notification window.
unsafe extern "system" fn device_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_DEVICECHANGE && wparam.0 == DBT_DEVICEREMOVECOMPLETE {
        DEVICE_REMOVED.store(true, Ordering::SeqCst);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Create a hidden message-only window and register it for Focusrite
/// device-interface removal notifications.
fn setup_device_notifications() {
    use std::mem;

    // Register a minimal window class with our device_wndproc.
    let class_name: Vec<u16> = "FocusMuteDevNotify\0".encode_utf16().collect();
    let wc = WNDCLASSEXW {
        cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(device_wndproc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..unsafe { mem::zeroed() }
    };
    if unsafe { RegisterClassExW(&wc) } == 0 {
        log::warn!("[hotplug] could not register window class");
        return;
    }

    // Create a message-only window (invisible, no taskbar entry).
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE), // message-only window
            None,
            None,
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        log::warn!("[hotplug] could not create notification window");
        return;
    };

    // Register for Focusrite device-interface notifications.
    let filter = DevBroadcastDeviceInterface {
        dbcc_size: mem::size_of::<DevBroadcastDeviceInterface>() as u32,
        dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
        dbcc_reserved: 0,
        dbcc_classguid: windows::core::GUID {
            data1: 0xAC4D0455,
            data2: 0x50D7,
            data3: 0x4498,
            data4: [0xB3, 0xCD, 0x9A, 0x41, 0xD1, 0x30, 0xB7, 0x59],
        },
        dbcc_name: [0],
    };
    let result = unsafe {
        RegisterDeviceNotificationW(
            HANDLE(hwnd.0),
            &filter as *const _ as *const std::ffi::c_void,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    };
    match result {
        Ok(_handle) => {
            log::info!("[hotplug] registered for Focusrite device notifications");
        }
        Err(e) => {
            log::warn!("[hotplug] RegisterDeviceNotification failed: {e}");
        }
    }
}

// ── Win32 message pump ──

/// Pump all pending Win32 messages. Required for tray-icon and global-hotkey
/// to receive their internal window messages on Windows.  Also dispatches
/// WM_DEVICECHANGE to the hidden hotplug window.
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

// ── Platform adapter ──

pub struct WindowsAdapter;

impl PlatformAdapter for WindowsAdapter {
    type Monitor = WasapiMonitor;

    fn platform_init() -> focusmute_lib::error::Result<()> {
        audio::com_init()?;
        Ok(())
    }

    fn create_monitor() -> Option<WasapiMonitor> {
        match WasapiMonitor::new() {
            Ok(m) => {
                log::info!("[audio] WASAPI mute monitor ready");
                Some(m)
            }
            Err(e) => {
                log::warn!("[audio] could not create mute monitor: {e}");
                None
            }
        }
    }

    fn spawn_poll_thread(monitor: Arc<WasapiMonitor>, tx: mpsc::Sender<Msg>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            if let Err(e) = audio::com_init() {
                log::error!("[audio] COM init error: {e}");
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

    fn register_device_notifications() {
        setup_device_notifications();
    }

    fn check_device_removed() -> bool {
        DEVICE_REMOVED.swap(false, Ordering::SeqCst)
    }
}

pub fn run() -> focusmute_lib::error::Result<()> {
    shared::run_core::<WindowsAdapter>()
}
