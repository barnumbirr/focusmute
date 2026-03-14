//! Global hotkey registration and management.

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};

/// Tracks the currently registered global hotkey.
pub struct HotkeyState {
    pub manager: GlobalHotKeyManager,
    pub current: HotKey,
    pub id: u32,
}

/// Parse and register the initial global hotkey.
///
/// Returns the hotkey state and whether registration succeeded.
pub fn register_hotkey(hotkey_str: &str) -> focusmute_lib::error::Result<(HotkeyState, bool)> {
    let manager = GlobalHotKeyManager::new().map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to init hotkey manager: {e}"))
    })?;
    let hotkey: HotKey = hotkey_str
        .parse()
        .unwrap_or_else(|_| "Ctrl+Shift+M".parse().unwrap());
    let id = hotkey.id();
    let registered = match manager.register(hotkey) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("[hotkey] could not register '{hotkey_str}': {e}");
            false
        }
    };
    Ok((
        HotkeyState {
            manager,
            current: hotkey,
            id,
        },
        registered,
    ))
}

/// Unregister the old hotkey and register a new one. Updates state in place.
///
/// Parses the new hotkey first so that the old one stays registered if the
/// new string is invalid.  If registering the new hotkey fails, the old one
/// is re-registered as a fallback.
///
/// Returns `true` on success, `false` if registration failed (old hotkey restored).
pub fn reregister_hotkey(hk: &mut HotkeyState, new_hotkey_str: &str) -> bool {
    let new_hk = match new_hotkey_str.parse::<HotKey>() {
        Ok(hk) => hk,
        Err(e) => {
            log::warn!("[config] invalid hotkey '{new_hotkey_str}': {e}");
            return false;
        }
    };
    if let Err(e) = hk.manager.unregister(hk.current) {
        log::warn!("[hotkey] could not unregister old hotkey: {e}");
    }
    if let Err(e) = hk.manager.register(new_hk) {
        log::warn!("[hotkey] could not register '{new_hotkey_str}': {e}");
        if let Err(e) = hk.manager.register(hk.current) {
            log::error!("[hotkey] could not restore previous hotkey: {e}");
        }
        return false;
    }
    hk.current = new_hk;
    hk.id = new_hk.id();
    true
}
