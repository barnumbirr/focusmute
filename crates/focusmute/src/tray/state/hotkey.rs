//! Global hotkey registration and management.

use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};

/// Tracks the currently registered global hotkeys (toggle + optional PTT).
pub struct HotkeyState {
    pub manager: GlobalHotKeyManager,
    pub toggle: HotKey,
    pub toggle_id: u32,
    /// Push-to-talk hotkey (hold to unmute, release to re-mute). `None` = disabled.
    pub ptt: Option<HotKey>,
    pub ptt_id: Option<u32>,
}

/// Parse and register the initial global hotkeys.
///
/// Returns the hotkey state and a list of warnings for any registration failures.
pub fn register_hotkeys(
    hotkey_str: &str,
    ptt_str: &str,
) -> focusmute_lib::error::Result<(HotkeyState, Vec<String>)> {
    let manager = GlobalHotKeyManager::new().map_err(|e| {
        focusmute_lib::FocusmuteError::Config(format!("Failed to init hotkey manager: {e}"))
    })?;
    let mut warnings = Vec::new();

    // Toggle hotkey (required)
    let toggle: HotKey = hotkey_str
        .parse()
        .unwrap_or_else(|_| "Ctrl+Shift+M".parse().unwrap());
    let toggle_id = toggle.id();
    if let Err(e) = manager.register(toggle) {
        log::warn!("[hotkey] could not register toggle '{hotkey_str}': {e}");
        warnings.push(format!(
            "Could not register hotkey \"{hotkey_str}\". It may be in use by another application."
        ));
    }

    // PTT hotkey (optional — empty string = disabled)
    let ptt_str = ptt_str.trim();
    let (ptt, ptt_id) = if ptt_str.is_empty() {
        (None, None)
    } else {
        match ptt_str.parse::<HotKey>() {
            Ok(hk) => {
                let id = hk.id();
                if let Err(e) = manager.register(hk) {
                    log::warn!("[hotkey] could not register PTT '{ptt_str}': {e}");
                    warnings.push(format!(
                        "Could not register push-to-talk hotkey \"{ptt_str}\". It may be in use by another application."
                    ));
                    (None, None)
                } else {
                    (Some(hk), Some(id))
                }
            }
            Err(e) => {
                log::warn!("[hotkey] invalid PTT hotkey '{ptt_str}': {e}");
                warnings.push(format!(
                    "Invalid push-to-talk hotkey \"{ptt_str}\". Using none."
                ));
                (None, None)
            }
        }
    };

    Ok((
        HotkeyState {
            manager,
            toggle,
            toggle_id,
            ptt,
            ptt_id,
        },
        warnings,
    ))
}

/// Unregister the old toggle hotkey and register a new one. Updates state in place.
///
/// Parses the new hotkey first so that the old one stays registered if the
/// new string is invalid.  If registering the new hotkey fails, the old one
/// is re-registered as a fallback.
///
/// Returns `true` on success, `false` if registration failed (old hotkey restored).
pub fn reregister_toggle(hk: &mut HotkeyState, new_hotkey_str: &str) -> bool {
    let new_hk = match new_hotkey_str.parse::<HotKey>() {
        Ok(hk) => hk,
        Err(e) => {
            log::warn!("[config] invalid hotkey '{new_hotkey_str}': {e}");
            return false;
        }
    };
    if let Err(e) = hk.manager.unregister(hk.toggle) {
        log::warn!("[hotkey] could not unregister old hotkey: {e}");
    }
    if let Err(e) = hk.manager.register(new_hk) {
        log::warn!("[hotkey] could not register '{new_hotkey_str}': {e}");
        if let Err(e) = hk.manager.register(hk.toggle) {
            log::error!("[hotkey] could not restore previous hotkey: {e}");
        }
        return false;
    }
    hk.toggle = new_hk;
    hk.toggle_id = new_hk.id();
    true
}

/// Update the PTT hotkey registration. Empty string disables PTT.
///
/// Returns `true` if the new PTT hotkey was registered (or disabled) successfully.
pub fn reregister_ptt(hk: &mut HotkeyState, new_ptt_str: &str) -> bool {
    // Unregister old PTT if present
    if let Some(old_ptt) = hk.ptt
        && let Err(e) = hk.manager.unregister(old_ptt)
    {
        log::warn!("[hotkey] could not unregister old PTT hotkey: {e}");
    }

    let new_ptt_str = new_ptt_str.trim();
    if new_ptt_str.is_empty() {
        hk.ptt = None;
        hk.ptt_id = None;
        return true;
    }

    let new_hk = match new_ptt_str.parse::<HotKey>() {
        Ok(hk) => hk,
        Err(e) => {
            log::warn!("[config] invalid PTT hotkey '{new_ptt_str}': {e}");
            hk.ptt = None;
            hk.ptt_id = None;
            return false;
        }
    };
    if let Err(e) = hk.manager.register(new_hk) {
        log::warn!("[hotkey] could not register PTT '{new_ptt_str}': {e}");
        hk.ptt = None;
        hk.ptt_id = None;
        return false;
    }
    hk.ptt = Some(new_hk);
    hk.ptt_id = Some(new_hk.id());
    true
}
