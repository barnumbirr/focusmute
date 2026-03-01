//! FocusMute — hotkey mute control for Focusrite Scarlett 4th Gen interfaces.

pub mod audio;
pub mod config;
pub mod context;
pub mod device;
pub mod error;
pub mod hooks;
pub mod layout;
pub mod led;
pub mod models;
pub mod monitor;
pub mod offsets;
pub mod protocol;
pub mod reconnect;
pub mod schema;

pub use error::FocusmuteError;
