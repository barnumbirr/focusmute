//! System tray — platform-specific event loops and shared state.

mod shared;
pub(crate) mod state;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
mod windows;

/// AppUserModelID for Windows toast notifications.
#[cfg(windows)]
pub(crate) const AUMID: &str = "Barnumbirr.Focusmute";

/// Register the AUMID in the Windows registry so toast notifications display
/// "FocusMute" with the app icon instead of "Windows PowerShell".
///
/// Writes to `HKCU\SOFTWARE\Classes\AppUserModelId\<AUMID>` with:
/// - `DisplayName` = "FocusMute"
/// - `IconUri` = path to icon PNG extracted to `%APPDATA%\Focusmute\`
///
/// Failures are silently ignored — worst case, notifications fall back to the
/// default PowerShell branding.
#[cfg(windows)]
fn register_aumid() {
    use std::path::PathBuf;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let Ok(appdata) = std::env::var("APPDATA") else {
        return;
    };

    // Extract the embedded icon to disk so Windows can reference it.
    let icon_dir = PathBuf::from(&appdata).join("Focusmute");
    let icon_path = icon_dir.join("icon.png");
    if !icon_path.exists() {
        let _ = std::fs::create_dir_all(&icon_dir);
        let _ = std::fs::write(&icon_path, crate::icon::ICON_PNG);
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok((key, _)) = hkcu.create_subkey(format!(r"SOFTWARE\Classes\AppUserModelId\{AUMID}"))
    else {
        return;
    };
    let _ = key.set_value("DisplayName", &"FocusMute");
    let _ = key.set_value("IconUri", &icon_path.to_string_lossy().to_string());
}

/// Enable dark mode for the system tray context menu on Windows.
///
/// Uses undocumented `uxtheme.dll` APIs (ordinal 135 = `SetPreferredAppMode`,
/// ordinal 136 = `FlushMenuThemes`) to opt into dark context menus. This is the
/// same technique used by Chrome, Firefox, and Windows Terminal.
///
/// Fails silently if the APIs are unavailable (e.g., older Windows versions).
#[cfg(windows)]
fn enable_dark_mode_menus() {
    use ::windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    /// `AllowDark` variant of the undocumented `PreferredAppMode` enum.
    const ALLOW_DARK: u32 = 1;

    // SAFETY: LoadLibraryW/GetProcAddress are standard Win32 FFI.
    // Ordinals 135/136 are well-known undocumented uxtheme.dll exports
    // (SetPreferredAppMode / FlushMenuThemes) used by Chrome, Firefox,
    // and Windows Terminal for dark context menus. The transmutes convert
    // non-null function pointers returned by GetProcAddress to the known
    // calling conventions of these exports.
    unsafe {
        let Ok(module) = LoadLibraryW(::windows::core::w!("uxtheme.dll")) else {
            return;
        };

        let set_mode = GetProcAddress(module, ::windows::core::PCSTR::from_raw(135 as *const u8));
        let flush = GetProcAddress(module, ::windows::core::PCSTR::from_raw(136 as *const u8));

        if let Some(f) = set_mode {
            let f: unsafe extern "system" fn(u32) -> u32 = std::mem::transmute(f);
            f(ALLOW_DARK);
        }
        if let Some(f) = flush {
            let f: unsafe extern "system" fn() = std::mem::transmute(f);
            f();
        }
    }
}

pub fn run() -> focusmute_lib::error::Result<()> {
    #[cfg(windows)]
    register_aumid();

    #[cfg(windows)]
    enable_dark_mode_menus();

    let instance = single_instance::SingleInstance::new("focusmute").map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to create instance lock: {e}"))
    })?;

    if !instance.is_single() {
        log::warn!("[focusmute] another instance is already running");
        crate::notification::Notifier::show_oneshot("Another instance is already running.");
        return Ok(());
    }

    // `instance` stays alive for the duration of run(), holding the lock.
    #[cfg(windows)]
    {
        windows::run()
    }

    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
}
