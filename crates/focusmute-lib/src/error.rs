//! Unified error type for the focusmute-lib crate.
//!
//! [`FocusmuteError`] wraps module-specific errors (`DeviceError`, `AudioError`)
//! and domain-specific error kinds (`Schema`, `Config`, `Layout`, `Color`).
//! `From` impls allow `?` to propagate across module boundaries seamlessly.

use std::fmt;

use crate::audio::AudioError;
use crate::device::DeviceError;

/// Unified error type for focusmute-lib operations.
#[derive(Debug)]
pub enum FocusmuteError {
    /// Device communication error (open, transact, descriptor I/O).
    Device(DeviceError),
    /// Audio subsystem error (COM init, mute monitor).
    Audio(AudioError),
    /// Standard I/O error (file read/write, config persistence).
    Io(std::io::Error),
    /// Schema extraction or parsing error.
    Schema(String),
    /// Configuration validation error.
    Config(String),
    /// LED layout prediction error.
    Layout(String),
    /// Color parsing error.
    Color(String),
}

impl fmt::Display for FocusmuteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FocusmuteError::Device(e) => write!(f, "{e}"),
            FocusmuteError::Audio(e) => write!(f, "{e}"),
            FocusmuteError::Io(e) => write!(f, "I/O error: {e}"),
            FocusmuteError::Schema(e) => write!(f, "Schema error: {e}"),
            FocusmuteError::Config(e) => write!(f, "Config error: {e}"),
            FocusmuteError::Layout(e) => write!(f, "Layout error: {e}"),
            FocusmuteError::Color(e) => write!(f, "Color error: {e}"),
        }
    }
}

impl std::error::Error for FocusmuteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FocusmuteError::Device(e) => Some(e),
            FocusmuteError::Audio(e) => Some(e),
            FocusmuteError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<DeviceError> for FocusmuteError {
    fn from(e: DeviceError) -> Self {
        FocusmuteError::Device(e)
    }
}

impl From<AudioError> for FocusmuteError {
    fn from(e: AudioError) -> Self {
        FocusmuteError::Audio(e)
    }
}

impl From<std::io::Error> for FocusmuteError {
    fn from(e: std::io::Error) -> Self {
        FocusmuteError::Io(e)
    }
}

/// Crate-level Result alias using [`FocusmuteError`].
pub type Result<T> = std::result::Result<T, FocusmuteError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_device_error() {
        let e: FocusmuteError = DeviceError::NotFound.into();
        assert!(matches!(e, FocusmuteError::Device(DeviceError::NotFound)));
    }

    #[test]
    fn from_audio_error() {
        let e: FocusmuteError = AudioError::InitFailed("test".into()).into();
        assert!(matches!(
            e,
            FocusmuteError::Audio(AudioError::InitFailed(_))
        ));
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let e: FocusmuteError = io_err.into();
        assert!(matches!(e, FocusmuteError::Io(_)));
    }

    #[test]
    fn display_device_error() {
        let e = FocusmuteError::Device(DeviceError::NotFound);
        assert_eq!(e.to_string(), "Scarlett device not found");
    }

    #[test]
    fn display_schema_error() {
        let e = FocusmuteError::Schema("bad json".into());
        assert_eq!(e.to_string(), "Schema error: bad json");
    }

    #[test]
    fn display_config_error() {
        let e = FocusmuteError::Config("invalid input".into());
        assert_eq!(e.to_string(), "Config error: invalid input");
    }

    #[test]
    fn display_layout_error() {
        let e = FocusmuteError::Layout("overflow".into());
        assert_eq!(e.to_string(), "Layout error: overflow");
    }

    #[test]
    fn display_color_error() {
        let e = FocusmuteError::Color("bad hex".into());
        assert_eq!(e.to_string(), "Color error: bad hex");
    }

    #[test]
    fn source_chains_device_error() {
        let e = FocusmuteError::Device(DeviceError::TransactFailed("timeout".into()));
        let source = std::error::Error::source(&e).unwrap();
        assert!(source.to_string().contains("timeout"));
    }

    #[test]
    fn source_chains_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e = FocusmuteError::Io(io_err);
        let source = std::error::Error::source(&e).unwrap();
        assert!(source.to_string().contains("denied"));
    }

    #[test]
    fn source_none_for_string_variants() {
        let e = FocusmuteError::Schema("test".into());
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn question_mark_propagation_device_to_focusmute() {
        fn inner() -> crate::device::Result<()> {
            Err(DeviceError::NotFound)
        }
        fn outer() -> Result<()> {
            inner()?;
            Ok(())
        }
        let err = outer().unwrap_err();
        assert!(matches!(err, FocusmuteError::Device(DeviceError::NotFound)));
    }

    #[test]
    fn question_mark_propagation_io_to_focusmute() {
        fn inner() -> std::io::Result<()> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "nope"))
        }
        fn outer() -> Result<()> {
            inner()?;
            Ok(())
        }
        let err = outer().unwrap_err();
        assert!(matches!(err, FocusmuteError::Io(_)));
    }
}
