//! System tray — platform-specific event loops and shared state.

mod shared;
pub(crate) mod state;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
mod windows;

/// AppUserModelID for Windows toast notifications.
#[cfg(windows)]
pub(super) const AUMID: &str = "Barnumbirr.Focusmute";

/// Register the AUMID in the Windows registry so toast notifications display
/// "Focusmute" with the app icon instead of "Windows PowerShell".
///
/// Writes to `HKCU\SOFTWARE\Classes\AppUserModelId\<AUMID>` with:
/// - `DisplayName` = "Focusmute"
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
    let _ = key.set_value("DisplayName", &"Focusmute");
    let _ = key.set_value("IconUri", &icon_path.to_string_lossy().to_string());
}

pub fn run() -> focusmute_lib::error::Result<()> {
    #[cfg(windows)]
    register_aumid();

    let instance = single_instance::SingleInstance::new("focusmute").map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to create instance lock: {e}"))
    })?;

    if !instance.is_single() {
        log::warn!("Another instance of Focusmute is already running.");
        let mut n = notify_rust::Notification::new();
        #[cfg(windows)]
        n.app_id(AUMID);
        #[cfg(target_os = "linux")]
        n.summary("Focusmute");
        n.body("Another instance is already running.");
        let _ = n.show();
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
