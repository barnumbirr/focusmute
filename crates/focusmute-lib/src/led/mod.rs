//! LED control — single-LED update, mute indicator apply/clear/restore.

mod color;
mod ops;
mod strategy;

pub use color::{format_color, parse_color};
pub use ops::{
    apply_mute_indicator, clear_mute_indicator, refresh_after_reconnect, restore_on_exit,
    set_single_led,
};
pub use strategy::{MuteStrategy, mute_color_or_default, resolve_strategy_from_config};
