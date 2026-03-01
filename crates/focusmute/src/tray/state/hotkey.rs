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
        log::warn!("could not register hotkey '{hotkey_str}': {e}");
    }
    Ok(HotkeyState {
        manager,
        current: hotkey,
        id,
    })
}

/// Unregister the old hotkey and register a new one. Updates state in place.
pub fn reregister_hotkey(hk: &mut HotkeyState, new_hotkey_str: &str) {
    let _ = hk.manager.unregister(hk.current);
    match new_hotkey_str.parse::<HotKey>() {
        Ok(new_hk) => {
            if let Err(e) = hk.manager.register(new_hk) {
                log::warn!("[config] could not register hotkey '{new_hotkey_str}': {e}");
            } else {
                hk.current = new_hk;
                hk.id = new_hk.id();
            }
        }
        Err(e) => {
            log::warn!("[config] invalid hotkey '{new_hotkey_str}': {e}");
        }
    }
}
