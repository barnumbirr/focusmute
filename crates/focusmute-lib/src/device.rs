//! Device communication — trait + Windows backend.

use std::fmt;

use serde::Serialize;

use crate::protocol::*;

// ── Error type ──

/// Device communication errors.
///
/// String payloads follow the convention **"context: details"** where *context*
/// identifies the operation or step (e.g. `"IOCTL_INIT"`, `"USB open"`) and
/// *details* describes what went wrong.  Bare descriptions (no colon) are
/// acceptable when no inner error is being wrapped.
#[derive(Debug)]
pub enum DeviceError {
    NotFound,
    OpenFailed(String),
    InitFailed(String),
    TransactFailed(String),
    UnsupportedDevice(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::NotFound => write!(f, "Scarlett device not found"),
            DeviceError::OpenFailed(e) => write!(f, "Failed to open device: {e}"),
            DeviceError::InitFailed(e) => write!(f, "Device init failed: {e}"),
            DeviceError::TransactFailed(e) => write!(f, "Transaction failed: {e}"),
            DeviceError::UnsupportedDevice(name) => {
                write!(
                    f,
                    "Unsupported device: {name} (no profile or schema available)"
                )
            }
        }
    }
}

impl std::error::Error for DeviceError {}

pub type Result<T> = std::result::Result<T, DeviceError>;

// ── Device info ──

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub path: String,
    /// Raw GET_CONFIG response (96 bytes).
    #[serde(skip)]
    pub config_raw: Vec<u8>,
    /// Raw USB_INIT response (up to 100 bytes).
    #[serde(skip)]
    pub init_raw: Vec<u8>,
    /// Device name from descriptor (offset 16, 32 bytes), e.g. "Scarlett 2i2 4th Gen-0003186a"
    pub device_name: String,
    /// Firmware version fields from descriptor header.
    pub firmware: FirmwareVersion,
    /// Serial number (from USB device instance ID, if available).
    pub serial: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FirmwareVersion {
    pub major: u16,
    pub minor: u16,
    pub stage_release: u32,
    pub build_nr: u32,
}

impl std::fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.stage_release, self.build_nr
        )
    }
}

impl FirmwareVersion {
    /// Parse firmware version from a 16-byte descriptor header.
    ///
    /// Layout: `[u32 unknown][u16 major][u16 minor][u32 stage_release][u32 build_nr]`
    pub fn from_descriptor_bytes(hdr: &[u8]) -> Self {
        if hdr.len() < 16 {
            return FirmwareVersion::default();
        }
        FirmwareVersion {
            major: u16::from_le_bytes(hdr[4..6].try_into().unwrap_or_default()),
            minor: u16::from_le_bytes(hdr[6..8].try_into().unwrap_or_default()),
            stage_release: u32::from_le_bytes(hdr[8..12].try_into().unwrap_or_default()),
            build_nr: u32::from_le_bytes(hdr[12..16].try_into().unwrap_or_default()),
        }
    }
}

/// Parse a null-terminated device name from raw descriptor bytes.
pub fn parse_device_name(name_bytes: &[u8]) -> String {
    let end = name_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name_bytes.len());
    String::from_utf8_lossy(&name_bytes[..end]).to_string()
}

impl DeviceInfo {
    pub fn token(&self) -> u64 {
        if self.config_raw.len() >= 16 {
            u64::from_le_bytes(self.config_raw[8..16].try_into().unwrap_or_default())
        } else {
            0
        }
    }

    /// Model name extracted from device_name (before the last dash-serial suffix).
    pub fn model(&self) -> &str {
        self.device_name
            .rsplit_once('-')
            .map(|(model, _)| model)
            .unwrap_or(&self.device_name)
            .trim()
    }
}

// ── Trait ──

pub trait ScarlettDevice {
    fn open() -> Result<Self>
    where
        Self: Sized;
    fn info(&self) -> &DeviceInfo;
    fn get_descriptor(&self, offset: u32, size: u32) -> Result<Vec<u8>>;
    fn set_descriptor(&self, offset: u32, data: &[u8]) -> Result<()>;
    fn data_notify(&self, event_id: u32) -> Result<()>;
    /// Send a raw TRANSACT command and return the response.
    fn transact(&self, cmd: u32, payload: &[u8], out_size: usize) -> Result<Vec<u8>>;

    /// Wait for a device notification (IOCTL_NOTIFY on Windows, USB interrupt on Linux).
    /// Returns notification data (typically 16 bytes) or times out.
    /// Default: not supported on this platform.
    fn wait_notify(&self, _timeout_ms: u64) -> Result<Vec<u8>> {
        Err(DeviceError::TransactFailed(
            "wait_notify not supported on this platform".into(),
        ))
    }

    /// Send a raw IOCTL (bypassing TRANSACT framing).
    /// Default: not supported on this platform.
    fn raw_ioctl(&self, _code: u32, _input: &[u8], _out_size: usize) -> Result<Vec<u8>> {
        Err(DeviceError::TransactFailed(
            "raw_ioctl not supported on this platform".into(),
        ))
    }
}

// ── Windows shared helpers ──

/// Shared SetupDi enumeration helpers used by both `WindowsDevice` and `enumerate_devices_windows`.
#[cfg(windows)]
mod win_enum {
    use crate::protocol::FOCUSRITE_GUID;
    use std::mem;
    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::core::PCWSTR;

    /// Extract a null-terminated UTF-16 path from SP_DEVICE_INTERFACE_DETAIL_DATA_W.
    ///
    /// # Safety
    /// `detail` must point to a valid, fully initialized SP_DEVICE_INTERFACE_DETAIL_DATA_W.
    pub unsafe fn extract_path(detail: &SP_DEVICE_INTERFACE_DETAIL_DATA_W) -> String {
        let ptr = &detail.DevicePath as *const u16;
        let mut len = 0;
        // SAFETY: caller guarantees `detail` is a valid, fully initialized struct.
        // We walk the null-terminated UTF-16 string within the DevicePath field.
        unsafe {
            while *ptr.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
        }
    }

    /// Enumerate all Focusrite `\pal` device interface paths.
    ///
    /// Calls `callback(path)` for each discovered path. If the callback returns
    /// `Some(T)`, enumeration stops and returns that value.
    pub fn enumerate_pal_paths<T>(mut callback: impl FnMut(String) -> Option<T>) -> Option<T> {
        unsafe {
            let dev_info = SetupDiGetClassDevsW(
                Some(&FOCUSRITE_GUID),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
            .ok()?;

            let result = enumerate_pal_paths_inner(dev_info, &mut callback);
            let _ = SetupDiDestroyDeviceInfoList(dev_info);
            result
        }
    }

    unsafe fn enumerate_pal_paths_inner<T>(
        dev_info: HDEVINFO,
        callback: &mut impl FnMut(String) -> Option<T>,
    ) -> Option<T> {
        for index in 0..32 {
            let mut iface = SP_DEVICE_INTERFACE_DATA {
                cbSize: mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            // SAFETY: dev_info is a valid handle from SetupDiGetClassDevsW.
            // iface is properly sized and initialized above.
            if unsafe {
                SetupDiEnumDeviceInterfaces(dev_info, None, &FOCUSRITE_GUID, index, &mut iface)
            }
            .is_err()
            {
                break;
            }
            let mut req: u32 = 0;
            // SAFETY: first call with NULL buffer to query required size.
            let _ = unsafe {
                SetupDiGetDeviceInterfaceDetailW(dev_info, &iface, None, 0, Some(&mut req), None)
            };
            if req == 0 {
                continue;
            }
            let mut buf = vec![0u8; req as usize];
            // SAFETY: buf is req bytes, large enough for the detail struct.
            let detail =
                unsafe { &mut *(buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W) };
            detail.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            // SAFETY: detail is properly sized (req bytes) and cbSize is set.
            if unsafe {
                SetupDiGetDeviceInterfaceDetailW(dev_info, &iface, Some(detail), req, None, None)
            }
            .is_ok()
            {
                // SAFETY: detail was just filled by SetupDiGetDeviceInterfaceDetailW.
                let path = unsafe { extract_path(detail) };
                if path.to_lowercase().ends_with("\\pal") {
                    if let Some(result) = callback(path) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }
}

// ── Windows implementation ──

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::mem;

    use windows::Win32::Devices::DeviceAndDriverInstallation::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::System::IO::{
        CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows::core::PCWSTR;

    /// Request/response pair for the I/O worker thread.
    struct IoctlRequest {
        ioctl: u32,
        input: Vec<u8>,
        out_size: usize,
        reply: std::sync::mpsc::Sender<std::result::Result<Vec<u8>, String>>,
    }

    pub struct WindowsDevice {
        handle: HANDLE,
        info: DeviceInfo,
        token: u64,
        /// Channel to the dedicated I/O worker thread.
        io_tx: std::sync::mpsc::Sender<IoctlRequest>,
    }

    // HANDLE is Send-safe for our usage (single-owner, sync I/O pattern)
    unsafe impl Send for WindowsDevice {}

    impl WindowsDevice {
        fn ioctl_sync(
            handle: HANDLE,
            ioctl: u32,
            input: &[u8],
            out_size: usize,
        ) -> std::result::Result<Vec<u8>, String> {
            let mut output = vec![0u8; out_size];
            let mut ret: u32 = 0;
            let in_ptr = if input.is_empty() {
                None
            } else {
                Some(input.as_ptr() as *const _)
            };
            let out_ptr = if out_size == 0 {
                None
            } else {
                Some(output.as_mut_ptr() as *mut _)
            };
            unsafe {
                DeviceIoControl(
                    handle,
                    ioctl,
                    in_ptr,
                    input.len() as u32,
                    out_ptr,
                    out_size as u32,
                    Some(&mut ret),
                    None,
                )
            }
            .map_err(|e| format!("{e}"))?;
            output.truncate(ret as usize);
            Ok(output)
        }

        /// Start a dedicated I/O worker thread for overlapped IOCTL calls.
        ///
        /// Returns the sender half of the channel. The worker runs until
        /// the sender is dropped (i.e., when WindowsDevice is dropped).
        fn spawn_io_worker(handle: HANDLE) -> std::sync::mpsc::Sender<IoctlRequest> {
            let (tx, rx) = std::sync::mpsc::channel::<IoctlRequest>();
            let handle_val = handle.0 as usize;
            std::thread::spawn(move || {
                let h = HANDLE(handle_val as *mut _);
                while let Ok(req) = rx.recv() {
                    let result = Self::ioctl_overlapped(h, req.ioctl, &req.input, req.out_size);
                    let _ = req.reply.send(result);
                }
            });
            tx
        }

        /// Send an IOCTL via the dedicated I/O worker thread with a 5-second timeout.
        fn ioctl_async(
            &self,
            ioctl: u32,
            input: &[u8],
            out_size: usize,
        ) -> std::result::Result<Vec<u8>, String> {
            use std::time::Duration;

            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            self.io_tx
                .send(IoctlRequest {
                    ioctl,
                    input: input.to_vec(),
                    out_size,
                    reply: reply_tx,
                })
                .map_err(|_| "I/O worker thread has exited".to_string())?;

            match reply_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(result) => result,
                Err(_) => {
                    // Timeout — cancel ALL pending I/O on this handle.
                    unsafe {
                        let _ = CancelIoEx(self.handle, None);
                    }
                    Err("IOCTL timed out after 5s".into())
                }
            }
        }

        /// Low-level overlapped DeviceIoControl. Blocks until completion.
        /// Called from the dedicated I/O worker thread.
        fn ioctl_overlapped(
            handle: HANDLE,
            ioctl: u32,
            input: &[u8],
            out_size: usize,
        ) -> std::result::Result<Vec<u8>, String> {
            let mut output = vec![0u8; out_size];
            let mut ret: u32 = 0;
            let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
                .map_err(|e| format!("CreateEvent: {e}"))?;
            let mut ov: OVERLAPPED = unsafe { mem::zeroed() };
            ov.hEvent = event;
            let in_ptr = if input.is_empty() {
                None
            } else {
                Some(input.as_ptr() as *const _)
            };
            let out_ptr = if out_size == 0 {
                None
            } else {
                Some(output.as_mut_ptr() as *mut _)
            };
            let r = unsafe {
                DeviceIoControl(
                    handle,
                    ioctl,
                    in_ptr,
                    input.len() as u32,
                    out_ptr,
                    out_size as u32,
                    Some(&mut ret),
                    Some(&mut ov),
                )
            };
            match r {
                Ok(()) => {}
                Err(e) if e.code().0 as u32 == 0x800703E5 => {
                    // ERROR_IO_PENDING — block until I/O completes (or is cancelled).
                    unsafe {
                        GetOverlappedResult(handle, &ov, &mut ret, true).map_err(|e2| {
                            let _ = CloseHandle(event);
                            format!("{e2}")
                        })?;
                    }
                }
                Err(e) => {
                    unsafe {
                        let _ = CloseHandle(event);
                    }
                    return Err(format!("{e}"));
                }
            }
            unsafe {
                let _ = CloseHandle(event);
            }
            output.truncate(ret as usize);
            Ok(output)
        }

        /// Low-level overlapped DeviceIoControl with configurable timeout.
        /// Used for IOCTL_NOTIFY which may pend indefinitely.
        /// Called directly (not from the I/O worker) to avoid blocking TRANSACT.
        fn ioctl_overlapped_timeout(
            handle: HANDLE,
            ioctl: u32,
            input: &[u8],
            out_size: usize,
            timeout_ms: u32,
        ) -> std::result::Result<Vec<u8>, String> {
            let mut output = vec![0u8; out_size];
            let mut ret: u32 = 0;
            let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
                .map_err(|e| format!("CreateEvent: {e}"))?;
            let mut ov: OVERLAPPED = unsafe { mem::zeroed() };
            ov.hEvent = event;
            let in_ptr = if input.is_empty() {
                None
            } else {
                Some(input.as_ptr() as *const _)
            };
            let out_ptr = if out_size == 0 {
                None
            } else {
                Some(output.as_mut_ptr() as *mut _)
            };
            let r = unsafe {
                DeviceIoControl(
                    handle,
                    ioctl,
                    in_ptr,
                    input.len() as u32,
                    out_ptr,
                    out_size as u32,
                    Some(&mut ret),
                    Some(&mut ov),
                )
            };
            match r {
                Ok(()) => {
                    // Completed synchronously
                    unsafe {
                        let _ = CloseHandle(event);
                    }
                    output.truncate(ret as usize);
                    Ok(output)
                }
                Err(e) if e.code().0 as u32 == 0x800703E5 => {
                    // ERROR_IO_PENDING — wait with timeout
                    let wait = unsafe { WaitForSingleObject(event, timeout_ms) };
                    match wait.0 {
                        0 => {
                            // WAIT_OBJECT_0 — completed
                            unsafe {
                                GetOverlappedResult(handle, &ov, &mut ret, false).map_err(
                                    |e2| {
                                        let _ = CloseHandle(event);
                                        format!("{e2}")
                                    },
                                )?;
                                let _ = CloseHandle(event);
                            }
                            output.truncate(ret as usize);
                            Ok(output)
                        }
                        0x102 => {
                            // WAIT_TIMEOUT — cancel the pending I/O
                            unsafe {
                                let _ = CancelIoEx(handle, Some(&ov));
                                // Wait for cancellation to complete
                                let _ = GetOverlappedResult(handle, &ov, &mut ret, true);
                                let _ = CloseHandle(event);
                            }
                            Err(format!("IOCTL timed out after {}ms", timeout_ms))
                        }
                        _ => {
                            // WAIT_FAILED or other
                            unsafe {
                                let _ = CancelIoEx(handle, Some(&ov));
                                let _ = GetOverlappedResult(handle, &ov, &mut ret, true);
                                let _ = CloseHandle(event);
                            }
                            Err("WaitForSingleObject failed".into())
                        }
                    }
                }
                Err(e) => {
                    unsafe {
                        let _ = CloseHandle(event);
                    }
                    Err(format!("{e}"))
                }
            }
        }

        fn transact_buf(token: u64, cmd: u32, payload: &[u8]) -> Vec<u8> {
            let mut buf = Vec::with_capacity(16 + payload.len());
            buf.extend_from_slice(&token.to_le_bytes());
            buf.extend_from_slice(&cmd.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(payload);
            buf
        }

        fn transact_impl(&self, cmd: u32, payload: &[u8], out_size: usize) -> Result<Vec<u8>> {
            let buf = Self::transact_buf(self.token, cmd, payload);
            self.ioctl_async(IOCTL_TRANSACT, &buf, out_size)
                .map_err(DeviceError::TransactFailed)
        }

        /// Find the \pal device path and USB serial number.
        fn find_device() -> Option<(String, Option<String>)> {
            super::win_enum::enumerate_pal_paths(|path| {
                let serial = Self::find_usb_serial();
                Some((path, serial))
            })
        }

        /// Find the Focusrite USB device serial by enumerating USB devices.
        fn find_usb_serial() -> Option<String> {
            // Search for USB devices with VID_1235 (Focusrite) in their instance ID
            let usb_enumerator: Vec<u16> = "USB".encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                let dev_info = SetupDiGetClassDevsW(
                    None,
                    PCWSTR(usb_enumerator.as_ptr()),
                    None,
                    DIGCF_ALLCLASSES | DIGCF_PRESENT,
                )
                .ok()?;
                for index in 0..256 {
                    let mut dev_data = SP_DEVINFO_DATA {
                        cbSize: mem::size_of::<SP_DEVINFO_DATA>() as u32,
                        ..Default::default()
                    };
                    if SetupDiEnumDeviceInfo(dev_info, index, &mut dev_data).is_err() {
                        break;
                    }
                    let mut instance_id = vec![0u16; 512];
                    if SetupDiGetDeviceInstanceIdW(
                        dev_info,
                        &dev_data,
                        Some(&mut instance_id),
                        None,
                    )
                    .is_ok()
                    {
                        let id = String::from_utf16_lossy(&instance_id);
                        let id = id.trim_end_matches('\0');
                        let id_upper = id.to_uppercase();
                        // Match Focusrite USB devices: USB\VID_1235&PID_xxxx\SERIAL
                        if id_upper.contains("VID_1235") {
                            // Serial is the third segment after the second backslash
                            let parts: Vec<&str> = id.split('\\').collect();
                            if parts.len() >= 3 && !parts[2].is_empty() {
                                let _ = SetupDiDestroyDeviceInfoList(dev_info);
                                return Some(parts[2].to_string());
                            }
                        }
                    }
                }
                let _ = SetupDiDestroyDeviceInfoList(dev_info);
            }
            None
        }
    }

    impl ScarlettDevice for WindowsDevice {
        fn open() -> Result<Self> {
            let (path, serial) = Self::find_device().ok_or(DeviceError::NotFound)?;

            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    None,
                )
            }
            .map_err(|e| DeviceError::OpenFailed(format!("CreateFileW: {e}")))?;

            // Init sequence (use ioctl_overlapped directly — worker not yet spawned)
            Self::ioctl_sync(handle, IOCTL_INIT, &[], 16)
                .map_err(|e| DeviceError::InitFailed(format!("IOCTL_INIT: {e}")))?;

            let init_buf = Self::transact_buf(0, CMD_USB_INIT, &[]);
            let init_raw = Self::ioctl_overlapped(handle, IOCTL_TRANSACT, &init_buf, 100)
                .map_err(|e| DeviceError::InitFailed(format!("USB_INIT: {e}")))?;

            let config_buf = Self::transact_buf(0, CMD_GET_CONFIG, &[]);
            let config_raw = Self::ioctl_overlapped(handle, IOCTL_TRANSACT, &config_buf, 96)
                .map_err(|e| DeviceError::InitFailed(format!("GET_CONFIG: {e}")))?;

            if config_raw.len() < 16 {
                return Err(DeviceError::InitFailed(
                    "GET_CONFIG response too short".into(),
                ));
            }

            let token = u64::from_le_bytes(config_raw[8..16].try_into().unwrap());

            // Spawn the dedicated I/O worker thread
            let io_tx = Self::spawn_io_worker(handle);

            // Create device with worker thread ready
            let mut dev = WindowsDevice {
                handle,
                info: DeviceInfo {
                    path,
                    config_raw,
                    init_raw,
                    device_name: String::new(),
                    firmware: FirmwareVersion::default(),
                    serial,
                },
                token,
                io_tx,
            };

            // Read firmware version from descriptor header (offset 0, 16 bytes)
            if let Ok(hdr) = dev.get_descriptor(0, 16) {
                dev.info.firmware = FirmwareVersion::from_descriptor_bytes(&hdr);
            }

            // Read device name from descriptor (offset 16, 32 bytes)
            if let Ok(name_bytes) = dev.get_descriptor(16, 32) {
                dev.info.device_name = parse_device_name(&name_bytes);
            }

            Ok(dev)
        }

        fn info(&self) -> &DeviceInfo {
            &self.info
        }

        fn get_descriptor(&self, offset: u32, size: u32) -> Result<Vec<u8>> {
            let mut payload = Vec::with_capacity(8);
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&size.to_le_bytes());
            let resp = self.transact_impl(CMD_GET_DESCR, &payload, (8 + size) as usize)?;
            // Response has 8-byte header, then data
            if resp.len() > 8 {
                Ok(resp[8..].to_vec())
            } else {
                Ok(resp)
            }
        }

        fn set_descriptor(&self, offset: u32, data: &[u8]) -> Result<()> {
            let mut payload = Vec::with_capacity(8 + data.len());
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
            payload.extend_from_slice(data);
            self.transact_impl(CMD_SET_DESCR, &payload, 8)?;
            Ok(())
        }

        fn data_notify(&self, event_id: u32) -> Result<()> {
            self.transact_impl(CMD_DATA_NOTIFY, &event_id.to_le_bytes(), 8)?;
            Ok(())
        }

        fn transact(&self, cmd: u32, payload: &[u8], out_size: usize) -> Result<Vec<u8>> {
            self.transact_impl(cmd, payload, out_size)
        }

        fn wait_notify(&self, timeout_ms: u64) -> Result<Vec<u8>> {
            // Direct overlapped I/O — bypasses the I/O worker thread
            // to avoid blocking concurrent TRANSACT operations.
            Self::ioctl_overlapped_timeout(self.handle, IOCTL_NOTIFY, &[], 16, timeout_ms as u32)
                .map_err(DeviceError::TransactFailed)
        }

        fn raw_ioctl(&self, code: u32, input: &[u8], out_size: usize) -> Result<Vec<u8>> {
            self.ioctl_async(code, input, out_size)
                .map_err(DeviceError::TransactFailed)
        }
    }

    impl Drop for WindowsDevice {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
pub use windows_impl::WindowsDevice;

// ── Linux implementation ──

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::time::Duration;

    use nusb::transfer::Control;
    use nusb::transfer::ControlType;
    use nusb::transfer::Recipient;

    use crate::protocol::{
        FOCUSRITE_VID, USB_BREQUEST_INIT, USB_BREQUEST_RX, USB_BREQUEST_TX, USB_CMD_INIT_1,
        USB_CMD_INIT_2, USB_HEADER_SIZE, USB_MAX_RETRIES, USB_TIMEOUT_MS, swroot_to_usb_cmd,
    };

    pub struct LinuxDevice {
        interface: nusb::Interface,
        info: DeviceInfo,
        seq: AtomicU16,
        interface_number: u16,
    }

    // nusb::Interface is Send-safe; AtomicU16 is Send+Sync.
    unsafe impl Send for LinuxDevice {}

    /// Build a raw USB packet: 16-byte header + payload.
    pub fn build_usb_packet(cmd: u32, seq: u16, payload: &[u8]) -> Vec<u8> {
        let size = payload.len() as u16;
        let mut pkt = Vec::with_capacity(USB_HEADER_SIZE + payload.len());
        pkt.extend_from_slice(&cmd.to_le_bytes()); // cmd: u32
        pkt.extend_from_slice(&size.to_le_bytes()); // size: u16
        pkt.extend_from_slice(&seq.to_le_bytes()); // seq: u16
        pkt.extend_from_slice(&0u32.to_le_bytes()); // error: u32
        pkt.extend_from_slice(&0u32.to_le_bytes()); // pad: u32
        pkt.extend_from_slice(payload);
        pkt
    }

    impl LinuxDevice {
        fn control_out(
            interface: &nusb::Interface,
            brequest: u8,
            windex: u16,
            data: &[u8],
        ) -> std::result::Result<(), String> {
            let control = Control {
                control_type: ControlType::Class,
                recipient: Recipient::Interface,
                request: brequest,
                value: 0,
                index: windex,
            };
            interface
                .control_out_blocking(control, data, Duration::from_millis(USB_TIMEOUT_MS))
                .map_err(|e| format!("control_out(bRequest={brequest}): {e}"))?;
            Ok(())
        }

        fn control_in(
            interface: &nusb::Interface,
            brequest: u8,
            windex: u16,
            length: usize,
        ) -> std::result::Result<Vec<u8>, String> {
            let control = Control {
                control_type: ControlType::Class,
                recipient: Recipient::Interface,
                request: brequest,
                value: 0,
                index: windex,
            };
            let mut buf = vec![0u8; length];
            let n = interface
                .control_in_blocking(control, &mut buf, Duration::from_millis(USB_TIMEOUT_MS))
                .map_err(|e| format!("control_in(bRequest={brequest}): {e}"))?;
            buf.truncate(n);
            Ok(buf)
        }

        /// Send a raw USB command and receive the response.
        ///
        /// Handles sequence counting and retry logic.
        /// Returns the response payload (after the 16-byte header).
        fn usb_transact(&self, cmd: u32, payload: &[u8], resp_size: usize) -> Result<Vec<u8>> {
            let seq = self.seq.load(Ordering::Relaxed);

            for attempt in 0..USB_MAX_RETRIES {
                let pkt = build_usb_packet(cmd, seq, payload);

                // TX
                Self::control_out(
                    &self.interface,
                    USB_BREQUEST_TX,
                    self.interface_number,
                    &pkt,
                )
                .map_err(DeviceError::TransactFailed)?;

                // RX — request header + expected response payload
                let rx_size = USB_HEADER_SIZE + resp_size;
                let resp = Self::control_in(
                    &self.interface,
                    USB_BREQUEST_RX,
                    self.interface_number,
                    rx_size,
                )
                .map_err(DeviceError::TransactFailed)?;

                if resp.len() < USB_HEADER_SIZE {
                    if attempt + 1 < USB_MAX_RETRIES {
                        let delay = 5 * (1 << attempt);
                        std::thread::sleep(Duration::from_millis(delay));
                        continue;
                    }
                    return Err(DeviceError::TransactFailed(format!(
                        "response too short: got {} bytes, need {USB_HEADER_SIZE}",
                        resp.len()
                    )));
                }

                // Parse response header
                let resp_cmd = u32::from_le_bytes(resp[0..4].try_into().unwrap());
                let resp_seq = u16::from_le_bytes(resp[6..8].try_into().unwrap());
                let resp_error = u32::from_le_bytes(resp[8..12].try_into().unwrap());

                // Validate command echo
                if resp_cmd != cmd {
                    return Err(DeviceError::TransactFailed(format!(
                        "command mismatch: sent 0x{cmd:08X}, got 0x{resp_cmd:08X}"
                    )));
                }

                // Validate sequence (init exception: req.seq==1 allows resp.seq==0)
                if resp_seq != seq && !(seq == 1 && resp_seq == 0) {
                    return Err(DeviceError::TransactFailed(format!(
                        "sequence mismatch: sent {seq}, got {resp_seq}"
                    )));
                }

                // Check error code
                if resp_error != 0 {
                    if attempt + 1 < USB_MAX_RETRIES {
                        let delay = 5 * (1 << attempt);
                        std::thread::sleep(Duration::from_millis(delay));
                        continue;
                    }
                    return Err(DeviceError::TransactFailed(format!(
                        "device error code: {resp_error}"
                    )));
                }

                // Success — increment sequence counter
                self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);

                // Return payload after header
                return Ok(resp[USB_HEADER_SIZE..].to_vec());
            }

            Err(DeviceError::TransactFailed("max retries exceeded".into()))
        }
    }

    impl ScarlettDevice for LinuxDevice {
        fn open() -> Result<Self> {
            // Find device
            let device_info = nusb::list_devices()
                .map_err(|e| DeviceError::OpenFailed(format!("USB enumeration: {e}")))?
                .find(|dev| dev.vendor_id() == FOCUSRITE_VID)
                .ok_or(DeviceError::NotFound)?;

            let serial = device_info.serial_number().map(|s| s.to_string());
            let product = device_info.product_string().unwrap_or_default().to_string();
            let bus_path = format!(
                "usb:{:03}/{:03}",
                device_info.bus_number(),
                device_info.device_address()
            );

            // Find the vendor-specific interface (bInterfaceClass == 255) from DeviceInfo
            let iface_num = device_info
                .interfaces()
                .find(|iface| iface.class() == 255)
                .map(|iface| iface.interface_number())
                .ok_or_else(|| DeviceError::OpenFailed("no vendor-specific interface".into()))?;

            let usb_device = device_info
                .open()
                .map_err(|e| DeviceError::OpenFailed(format!("USB open: {e}")))?;

            // Claim interface (nusb auto-detaches kernel driver)
            let interface = usb_device.claim_interface(iface_num).map_err(|e| {
                DeviceError::OpenFailed(format!("claim interface {iface_num}: {e}"))
            })?;

            let windex = iface_num as u16;

            // Step 0 — "cargo cult" init read (bRequest=0, 24 bytes)
            let _ = Self::control_in(&interface, USB_BREQUEST_INIT, windex, 24);

            // Sleep 20ms to let pending ACKs drain
            std::thread::sleep(Duration::from_millis(20));

            // Create device with placeholder info
            let mut dev = LinuxDevice {
                interface,
                info: DeviceInfo {
                    path: bus_path,
                    config_raw: vec![],
                    init_raw: vec![],
                    device_name: product,
                    firmware: FirmwareVersion::default(),
                    serial,
                },
                seq: AtomicU16::new(1), // set to 1 before init steps
                interface_number: windex,
            };

            // Step 1 — INIT_1 (cmd=0x00000000, seq=1)
            dev.usb_transact(USB_CMD_INIT_1, &[], 0)
                .map_err(|e| DeviceError::InitFailed(format!("INIT_1: {e}")))?;

            // Step 2 — INIT_2 (cmd=0x00000002, seq incremented)
            let init2_resp = dev
                .usb_transact(USB_CMD_INIT_2, &[], 84)
                .map_err(|e| DeviceError::InitFailed(format!("INIT_2: {e}")))?;

            // Save INIT_2 response (needed by DeviceInfo for token, etc.)
            dev.info.init_raw = init2_resp;

            // Read firmware version from descriptor header (offset 0, 16 bytes)
            if let Ok(hdr) = dev.get_descriptor(0, 16) {
                dev.info.firmware = FirmwareVersion::from_descriptor_bytes(&hdr);
            }

            // Read device name from descriptor (offset 16, 32 bytes)
            if let Ok(name_bytes) = dev.get_descriptor(16, 32) {
                dev.info.device_name = parse_device_name(&name_bytes);
            }

            Ok(dev)
        }

        fn info(&self) -> &DeviceInfo {
            &self.info
        }

        fn get_descriptor(&self, offset: u32, size: u32) -> Result<Vec<u8>> {
            let usb_cmd =
                swroot_to_usb_cmd(CMD_GET_DESCR).expect("CMD_GET_DESCR must have USB mapping");
            let mut payload = Vec::with_capacity(8);
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&size.to_le_bytes());
            self.usb_transact(usb_cmd, &payload, size as usize)
        }

        fn set_descriptor(&self, offset: u32, data: &[u8]) -> Result<()> {
            let usb_cmd =
                swroot_to_usb_cmd(CMD_SET_DESCR).expect("CMD_SET_DESCR must have USB mapping");
            let mut payload = Vec::with_capacity(8 + data.len());
            payload.extend_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
            payload.extend_from_slice(data);
            self.usb_transact(usb_cmd, &payload, 0)?;
            Ok(())
        }

        fn data_notify(&self, event_id: u32) -> Result<()> {
            let usb_cmd =
                swroot_to_usb_cmd(CMD_DATA_NOTIFY).expect("CMD_DATA_NOTIFY must have USB mapping");
            self.usb_transact(usb_cmd, &event_id.to_le_bytes(), 0)?;
            Ok(())
        }

        fn transact(&self, cmd: u32, payload: &[u8], out_size: usize) -> Result<Vec<u8>> {
            let usb_cmd = swroot_to_usb_cmd(cmd).ok_or_else(|| {
                DeviceError::TransactFailed(format!(
                    "no USB mapping for SwRoot command 0x{cmd:08X}"
                ))
            })?;

            let raw_resp = self.usb_transact(usb_cmd, payload, out_size)?;

            // Return [8 zero bytes] + raw response to match the Windows TRANSACT
            // format where responses have an 8-byte header before data.
            // This ensures schema.rs parsing (info_resp[10..12] for config_len) works
            // unchanged across both backends.
            let mut compat = vec![0u8; 8];
            compat.extend_from_slice(&raw_resp);
            Ok(compat)
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::LinuxDevice;

// ── Stub device for unsupported platforms ──

/// Placeholder device that always returns `NotFound`.
/// Enables compilation and `cargo test` on unsupported hosts.
#[cfg(not(any(windows, target_os = "linux")))]
pub struct StubDevice;

#[cfg(not(any(windows, target_os = "linux")))]
impl ScarlettDevice for StubDevice {
    fn open() -> Result<Self> {
        Err(DeviceError::NotFound)
    }
    fn info(&self) -> &DeviceInfo {
        unreachable!()
    }
    fn get_descriptor(&self, _offset: u32, _size: u32) -> Result<Vec<u8>> {
        unreachable!()
    }
    fn set_descriptor(&self, _offset: u32, _data: &[u8]) -> Result<()> {
        unreachable!()
    }
    fn data_notify(&self, _event_id: u32) -> Result<()> {
        unreachable!()
    }
    fn transact(&self, _cmd: u32, _payload: &[u8], _out_size: usize) -> Result<Vec<u8>> {
        unreachable!()
    }
}

// ── Device enumeration ──

/// A discovered Focusrite device interface (not yet opened/initialized).
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDevice {
    /// Device interface path (e.g., `\\?\...#pal`).
    pub path: String,
    /// USB serial number, if available.
    pub serial: Option<String>,
}

/// Enumerate all Focusrite device interfaces.
///
/// Returns a list of discovered devices without opening or initializing them.
/// On unsupported platforms, always returns an empty list.
pub fn enumerate_devices() -> Vec<DiscoveredDevice> {
    #[cfg(windows)]
    {
        enumerate_devices_windows()
    }
    #[cfg(target_os = "linux")]
    {
        enumerate_devices_linux()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
fn enumerate_devices_windows() -> Vec<DiscoveredDevice> {
    let mut devices = Vec::new();
    win_enum::enumerate_pal_paths(|path| {
        devices.push(DiscoveredDevice {
            path,
            serial: None, // serial requires additional USB enumeration
        });
        None::<()> // continue enumerating
    });
    devices
}

#[cfg(target_os = "linux")]
fn enumerate_devices_linux() -> Vec<DiscoveredDevice> {
    use crate::protocol::FOCUSRITE_VID;

    let Ok(devices) = nusb::list_devices() else {
        return Vec::new();
    };

    devices
        .filter(|dev| dev.vendor_id() == FOCUSRITE_VID)
        .filter(|dev| {
            // Only include devices with a vendor-specific interface (class 255)
            dev.interfaces().any(|iface| iface.class() == 255)
        })
        .map(|dev| {
            let path = format!(
                "usb:{:03}/{:03} [{:04x}:{:04x}]",
                dev.bus_number(),
                dev.device_address(),
                dev.vendor_id(),
                dev.product_id(),
            );
            let serial = dev.serial_number().map(|s| s.to_string());
            DiscoveredDevice { path, serial }
        })
        .collect()
}

/// Concrete device type for the current platform.
///
/// Use this when you need to name the device type explicitly (e.g. storing
/// in a struct field or returning from a helper function).  Prefer
/// `open_device()` + `impl ScarlettDevice` for most call sites.
#[cfg(windows)]
pub type PlatformDevice = WindowsDevice;
#[cfg(target_os = "linux")]
pub type PlatformDevice = LinuxDevice;
#[cfg(not(any(windows, target_os = "linux")))]
pub type PlatformDevice = StubDevice;

/// Open the platform-appropriate Scarlett device.
pub fn open_device() -> Result<PlatformDevice> {
    PlatformDevice::open()
}

/// Open a device matching the given serial number.
///
/// If `serial` is empty, delegates to [`open_device`] (auto-select).
/// Otherwise enumerates devices and opens the one with a matching serial.
pub fn open_device_by_serial(serial: &str) -> Result<PlatformDevice> {
    let serial = serial.trim();
    if serial.is_empty() {
        return open_device();
    }
    let devices = enumerate_devices();
    let matched = devices.iter().find(|d| {
        d.serial
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case(serial))
    });
    if let Some(matched_dev) = matched {
        // Verify the matched device is the first one — multi-device selection
        // (path-based open) isn't yet supported, so reject ambiguous cases.
        if devices.len() > 1 && devices[0].path != matched_dev.path {
            return Err(DeviceError::OpenFailed(format!(
                "device with serial '{serial}' found but multi-device selection is not yet supported \
                 (found {} devices; matched device is not the first)",
                devices.len()
            )));
        }
        open_device()
    } else if devices.is_empty() {
        Err(DeviceError::NotFound)
    } else {
        let available: Vec<String> = devices
            .iter()
            .map(|d| d.serial.as_deref().unwrap_or("(no serial)").to_string())
            .collect();
        Err(DeviceError::OpenFailed(format!(
            "no device with serial '{serial}' found (available: {})",
            available.join(", ")
        )))
    }
}

// ── Mock device for testing ──

/// In-memory mock device for unit and integration tests.
///
/// Always compiled (zero runtime cost), hidden from public docs.
#[doc(hidden)]
pub mod mock {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    /// In-memory device for unit tests. Stores descriptor data in a HashMap
    /// keyed by offset; `set_descriptor` writes, `get_descriptor` reads.
    /// `transact_handlers` allows tests to inject mock responses for raw transact calls.
    pub struct MockDevice {
        info: DeviceInfo,
        /// Descriptor storage: offset → data bytes.
        pub descriptors: RefCell<HashMap<u32, Vec<u8>>>,
        /// Recorded notify events (event_id).
        pub notifies: RefCell<Vec<u32>>,
        /// Mock transact responses: cmd → response bytes.
        pub transact_handlers: RefCell<HashMap<u32, Vec<Vec<u8>>>>,
        /// Recorded transact calls: (cmd, payload).
        pub transact_payloads: RefCell<Vec<(u32, Vec<u8>)>>,
        /// If set, `get_descriptor` truncates results to this many bytes.
        pub get_descriptor_max_len: Cell<Option<usize>>,
        /// If true, `set_descriptor` returns an error.
        pub fail_set_descriptor: Cell<bool>,
    }

    impl Default for MockDevice {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockDevice {
        pub fn new() -> Self {
            MockDevice {
                info: DeviceInfo {
                    path: "mock://scarlett-2i2".into(),
                    config_raw: vec![0; 96],
                    init_raw: vec![0; 100],
                    device_name: "Scarlett 2i2 4th Gen-00031337".into(),
                    firmware: FirmwareVersion {
                        major: 1,
                        minor: 2,
                        stage_release: 3,
                        build_nr: 4,
                    },
                    serial: Some("MOCK123".into()),
                },
                descriptors: RefCell::new(HashMap::new()),
                notifies: RefCell::new(Vec::new()),
                transact_handlers: RefCell::new(HashMap::new()),
                transact_payloads: RefCell::new(Vec::new()),
                get_descriptor_max_len: Cell::new(None),
                fail_set_descriptor: Cell::new(false),
            }
        }

        /// Mutable access to device info (for tests that need a different model name).
        pub fn info_mut(&mut self) -> &mut DeviceInfo {
            &mut self.info
        }

        /// Register a sequence of responses for a given command code.
        /// Each call to `transact()` with this cmd pops the first response.
        pub fn add_transact_response(&self, cmd: u32, response: Vec<u8>) {
            self.transact_handlers
                .borrow_mut()
                .entry(cmd)
                .or_default()
                .push(response);
        }
    }

    impl ScarlettDevice for MockDevice {
        fn open() -> Result<Self> {
            Ok(Self::new())
        }

        fn info(&self) -> &DeviceInfo {
            &self.info
        }

        fn get_descriptor(&self, offset: u32, size: u32) -> Result<Vec<u8>> {
            let descs = self.descriptors.borrow();
            let mut result = vec![0u8; size as usize];
            // Overlay all stored regions that intersect [offset..offset+size)
            for (&stored_offset, data) in descs.iter() {
                let stored_end = stored_offset + data.len() as u32;
                let req_end = offset + size;
                // Check for overlap
                if stored_offset < req_end && stored_end > offset {
                    let src_start = offset.saturating_sub(stored_offset) as usize;
                    let dst_start = stored_offset.saturating_sub(offset) as usize;
                    let copy_len = (data.len() - src_start).min(result.len() - dst_start);
                    result[dst_start..dst_start + copy_len]
                        .copy_from_slice(&data[src_start..src_start + copy_len]);
                }
            }
            // Truncate if test requested a short response
            if let Some(max) = self.get_descriptor_max_len.get() {
                result.truncate(max);
            }
            Ok(result)
        }

        fn set_descriptor(&self, offset: u32, data: &[u8]) -> Result<()> {
            if self.fail_set_descriptor.get() {
                return Err(DeviceError::TransactFailed(
                    "mock: set_descriptor failure injected".into(),
                ));
            }
            self.descriptors.borrow_mut().insert(offset, data.to_vec());
            Ok(())
        }

        fn data_notify(&self, event_id: u32) -> Result<()> {
            self.notifies.borrow_mut().push(event_id);
            Ok(())
        }

        fn transact(&self, cmd: u32, payload: &[u8], _out_size: usize) -> Result<Vec<u8>> {
            self.transact_payloads
                .borrow_mut()
                .push((cmd, payload.to_vec()));
            let mut handlers = self.transact_handlers.borrow_mut();
            if let Some(responses) = handlers.get_mut(&cmd)
                && !responses.is_empty()
            {
                return Ok(responses.remove(0));
            }
            Err(DeviceError::TransactFailed(format!(
                "no mock handler for cmd 0x{cmd:08X}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Serialization ──

    #[test]
    fn device_info_serializes_without_raw_fields() {
        let info = DeviceInfo {
            path: "test://path".into(),
            config_raw: vec![0xDE, 0xAD],
            init_raw: vec![0xBE, 0xEF],
            device_name: "Scarlett 2i2 4th Gen-00031337".into(),
            firmware: FirmwareVersion {
                major: 2,
                minor: 0,
                stage_release: 2417,
                build_nr: 0,
            },
            serial: Some("ABC123".into()),
        };
        let json = serde_json::to_string(&info).expect("serialize DeviceInfo");
        assert!(json.contains("\"path\""), "should contain path");
        assert!(
            json.contains("\"device_name\""),
            "should contain device_name"
        );
        assert!(json.contains("\"firmware\""), "should contain firmware");
        assert!(json.contains("\"serial\""), "should contain serial");
        assert!(!json.contains("config_raw"), "should skip config_raw");
        assert!(!json.contains("init_raw"), "should skip init_raw");
    }

    #[test]
    fn discovered_device_serializes() {
        let d = DiscoveredDevice {
            path: r"\\?\usb#vid_1235&pid_8215#pal".into(),
            serial: Some("ABCD1234".into()),
        };
        let json = serde_json::to_string(&d).expect("serialize DiscoveredDevice");
        assert!(json.contains("\"path\""));
        assert!(json.contains("\"serial\""));
        assert!(json.contains("ABCD1234"));
    }

    // ── FirmwareVersion ──

    #[test]
    fn firmware_version_to_string() {
        let v = FirmwareVersion {
            major: 1,
            minor: 2,
            stage_release: 345,
            build_nr: 6789,
        };
        assert_eq!(v.to_string(), "1.2.345.6789");
    }

    #[test]
    fn firmware_version_default_is_zeroes() {
        let v = FirmwareVersion::default();
        assert_eq!(v.to_string(), "0.0.0.0");
    }

    // ── DeviceInfo::token ──

    #[test]
    fn token_from_valid_config() {
        let mut config = vec![0u8; 96];
        // Write a known token at bytes 8..16
        config[8..16].copy_from_slice(&0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes());
        let info = DeviceInfo {
            path: String::new(),
            config_raw: config,
            init_raw: vec![],
            device_name: String::new(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.token(), 0xDEAD_BEEF_CAFE_BABE);
    }

    #[test]
    fn token_from_short_config_returns_zero() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![0u8; 8], // too short
            init_raw: vec![],
            device_name: String::new(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.token(), 0);
    }

    #[test]
    fn token_from_empty_config_returns_zero() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: String::new(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.token(), 0);
    }

    // ── DeviceInfo::model ──

    #[test]
    fn model_extracts_before_dash() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: "Scarlett 2i2 4th Gen-0003186a".into(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "Scarlett 2i2 4th Gen");
    }

    #[test]
    fn model_no_dash_returns_full_name() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: "Scarlett Solo".into(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "Scarlett Solo");
    }

    #[test]
    fn model_empty_name() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: String::new(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "");
    }

    #[test]
    fn model_trims_whitespace() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: "  Scarlett 2i2  -serial".into(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "Scarlett 2i2");
    }

    #[test]
    fn model_with_hyphenated_name() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: "Scarlett 4i4-Pro-0003186a".into(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "Scarlett 4i4-Pro");
    }

    #[test]
    fn model_dash_only() {
        let info = DeviceInfo {
            path: String::new(),
            config_raw: vec![],
            init_raw: vec![],
            device_name: "-".into(),
            firmware: FirmwareVersion::default(),
            serial: None,
        };
        assert_eq!(info.model(), "");
    }

    // ── enumerate_devices ──

    // ── FirmwareVersion::from_descriptor_bytes ──

    #[test]
    fn firmware_from_descriptor_bytes_valid() {
        let mut hdr = [0u8; 16];
        // major=2 at offset 4-5
        hdr[4..6].copy_from_slice(&2u16.to_le_bytes());
        // minor=0 at offset 6-7
        hdr[6..8].copy_from_slice(&0u16.to_le_bytes());
        // stage_release=2417 at offset 8-11
        hdr[8..12].copy_from_slice(&2417u32.to_le_bytes());
        // build_nr=0 at offset 12-15
        hdr[12..16].copy_from_slice(&0u32.to_le_bytes());

        let fw = FirmwareVersion::from_descriptor_bytes(&hdr);
        assert_eq!(fw.major, 2);
        assert_eq!(fw.minor, 0);
        assert_eq!(fw.stage_release, 2417);
        assert_eq!(fw.build_nr, 0);
        assert_eq!(fw.to_string(), "2.0.2417.0");
    }

    #[test]
    fn firmware_from_descriptor_bytes_short() {
        let hdr = [0u8; 8]; // too short
        let fw = FirmwareVersion::from_descriptor_bytes(&hdr);
        assert_eq!(fw.to_string(), "0.0.0.0");
    }

    #[test]
    fn firmware_from_descriptor_bytes_empty() {
        let fw = FirmwareVersion::from_descriptor_bytes(&[]);
        assert_eq!(fw.to_string(), "0.0.0.0");
    }

    // ── parse_device_name ──

    #[test]
    fn parse_device_name_null_terminated() {
        let mut bytes = [0u8; 32];
        let name = b"Scarlett 2i2 4th Gen-0003186a";
        bytes[..name.len()].copy_from_slice(name);
        assert_eq!(parse_device_name(&bytes), "Scarlett 2i2 4th Gen-0003186a");
    }

    #[test]
    fn parse_device_name_no_null() {
        let bytes = b"Hello World";
        assert_eq!(parse_device_name(bytes), "Hello World");
    }

    #[test]
    fn parse_device_name_empty() {
        assert_eq!(parse_device_name(&[]), "");
    }

    #[test]
    fn parse_device_name_all_nulls() {
        assert_eq!(parse_device_name(&[0, 0, 0, 0]), "");
    }

    // ── USB packet building (Linux) ──

    #[cfg(target_os = "linux")]
    #[test]
    fn build_usb_packet_no_payload() {
        let pkt = linux_impl::build_usb_packet(0x0080_0000, 5, &[]);
        assert_eq!(pkt.len(), 16);
        // cmd
        assert_eq!(
            u32::from_le_bytes(pkt[0..4].try_into().unwrap()),
            0x0080_0000
        );
        // size
        assert_eq!(u16::from_le_bytes(pkt[4..6].try_into().unwrap()), 0);
        // seq
        assert_eq!(u16::from_le_bytes(pkt[6..8].try_into().unwrap()), 5);
        // error
        assert_eq!(u32::from_le_bytes(pkt[8..12].try_into().unwrap()), 0);
        // pad
        assert_eq!(u32::from_le_bytes(pkt[12..16].try_into().unwrap()), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn build_usb_packet_with_payload() {
        let payload = [0x01, 0x02, 0x03, 0x04];
        let pkt = linux_impl::build_usb_packet(0x0080_0001, 10, &payload);
        assert_eq!(pkt.len(), 20); // 16 header + 4 payload
        assert_eq!(u16::from_le_bytes(pkt[4..6].try_into().unwrap()), 4);
        assert_eq!(&pkt[16..], &payload);
    }

    // ── enumerate_devices ──

    #[test]
    fn enumerate_devices_returns_vec() {
        // On test host: returns vec (possibly empty) — no panic, no error.
        let devices = enumerate_devices();
        assert!(devices.is_empty() || !devices.is_empty()); // type check
    }

    #[test]
    fn discovered_device_struct() {
        let d = DiscoveredDevice {
            path: r"\\?\usb#vid_1235&pid_8215#pal".into(),
            serial: Some("ABCD1234".into()),
        };
        assert!(d.path.contains("pal"));
        assert_eq!(d.serial.as_deref(), Some("ABCD1234"));
        // Clone + Debug
        let d2 = d.clone();
        assert_eq!(d2.path, d.path);
        let _ = format!("{d:?}");
    }
}
