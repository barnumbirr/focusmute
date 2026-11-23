//! LED control — single-LED update, mute indicator apply/clear/restore.

mod color;
mod ops;
mod strategy;

pub use color::{color_to_rgb, format_color, parse_color, rgb_to_hex};
pub use ops::{
    apply_mute_indicator, clear_mute_indicator, persist_saved_gains, refresh_after_reconnect,
    restore_on_exit, restore_preamp_gain, set_single_led,
};
pub use strategy::{MuteStrategy, mute_color_or_default, resolve_strategy_from_config};
