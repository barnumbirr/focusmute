//! Global hotkey registration and management.

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};

/// Tracks the currently registered global hotkey.
pub struct HotkeyState {
    pub manager: GlobalHotKeyManager,
    pub current: HotKey,
    pub id: u32,
}

/// Parse and register the initial global hotkey.
pub fn register_hotkey(hotkey_str: &str) -> focusmute_lib::error::Result<HotkeyState> {
    let manager = GlobalHotKeyManager::new().map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to init hotkey manager: {e}"))
    })?;
    let hotkey: HotKey = hotkey_str
        .parse()
        .unwrap_or_else(|_| "Ctrl+Shift+M".parse().unwrap());
    let id = hotkey.id();
    if let Err(e) = manager.register(hotkey) {
        log::warn!("[hotkey] could not register '{hotkey_str}': {e}");
    }
    Ok(HotkeyState {
        manager,
        current: hotkey,
        id,
    })
}

/// Unregister the old hotkey and register a new one. Updates state in place.
///
/// Parses the new hotkey first so that the old one stays registered if the
/// new string is invalid.  If registering the new hotkey fails, the old one
/// is re-registered as a fallback.
pub fn reregister_hotkey(hk: &mut HotkeyState, new_hotkey_str: &str) {
    let new_hk = match new_hotkey_str.parse::<HotKey>() {
        Ok(hk) => hk,
        Err(e) => {
            log::warn!("[config] invalid hotkey '{new_hotkey_str}': {e}");
            return; // old hotkey stays registered
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
    } else {
        hk.current = new_hk;
        hk.id = new_hk.id();
    }
}
