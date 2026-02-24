//! Device context — resolves model profile, schema, offsets, and predicted layout.
//!
//! Consolidates the repeated resolution pattern used by CLI commands and the
//! tray app: detect profile → extract schema → compute offsets → predict layout.

use crate::device::{DeviceError, ScarlettDevice};
use crate::layout::{self, PredictedLayout};
use crate::models::{self, ModelProfile};
use crate::offsets::DeviceOffsets;
use crate::schema::{self, SchemaConstants};

/// Resolved device context with model profile, schema, offsets, and layout.
#[derive(Debug)]
pub struct DeviceContext {
    pub profile: Option<&'static ModelProfile>,
    pub schema: Option<SchemaConstants>,
    pub offsets: DeviceOffsets,
    pub predicted: Option<PredictedLayout>,
}

impl DeviceContext {
    /// Resolve context from a connected device.
    ///
    /// If `force_schema` is false (the common case), schema extraction is
    /// skipped when a hardcoded profile exists — avoiding a multi-second USB
    /// round-trip on first run with known devices.
    ///
    /// Returns `Err(UnsupportedDevice)` if no profile exists and schema
    /// extraction also failed — the device cannot be operated safely.
    pub fn resolve(device: &impl ScarlettDevice, force_schema: bool) -> crate::error::Result<Self> {
        let profile = models::detect_model(device.info().model());

        let schema = if force_schema || profile.is_none() {
            schema::extract_or_cached(device).ok()
        } else {
            None
        };

        let offsets = if let Some(ref sc) = schema {
            DeviceOffsets::from_schema(sc)
        } else if profile.is_none() {
            return Err(DeviceError::UnsupportedDevice(device.info().model().to_string()).into());
        } else {
            // Known profile but no schema — safe to use defaults
            DeviceOffsets::default()
        };

        let predicted = if profile.is_none() {
            schema
                .as_ref()
                .and_then(|sc| layout::predict_layout(sc).ok())
        } else {
            None
        };

        Ok(DeviceContext {
            profile,
            schema,
            offsets,
            predicted,
        })
    }

    /// The effective input count from the best available source.
    pub fn input_count(&self) -> Option<usize> {
        self.profile
            .map(|p| p.input_count)
            .or_else(|| self.predicted.as_ref().map(|pl| pl.input_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::mock::MockDevice;

    fn mock_with_name(name: &str) -> MockDevice {
        let mut dev = MockDevice::new();
        dev.info_mut().device_name = name.into();
        dev
    }

    #[test]
    fn known_model_skips_schema() {
        let dev = mock_with_name("Scarlett 2i2 4th Gen-00031337");
        let ctx = DeviceContext::resolve(&dev, false).unwrap();
        assert!(ctx.profile.is_some());
        assert_eq!(ctx.profile.unwrap().name, "Scarlett 2i2 4th Gen");
        assert!(
            ctx.schema.is_none(),
            "schema should be skipped for known model"
        );
        assert!(
            ctx.predicted.is_none(),
            "predicted should be None when profile exists"
        );
    }

    #[test]
    fn known_model_force_schema_still_has_profile() {
        let dev = mock_with_name("Scarlett 2i2 4th Gen-00031337");
        let ctx = DeviceContext::resolve(&dev, true).unwrap();
        assert!(ctx.profile.is_some());
        // Schema extraction will fail on mock but profile is still detected
    }

    #[test]
    fn unknown_model_no_schema_returns_error() {
        let dev = mock_with_name("Scarlett Solo 4th Gen-00031337");
        let err = DeviceContext::resolve(&dev, false).unwrap_err();
        assert!(
            err.to_string().contains("Unsupported device"),
            "expected UnsupportedDevice error, got: {err}"
        );
    }

    #[test]
    fn input_count_from_profile() {
        let dev = mock_with_name("Scarlett 2i2 4th Gen-00031337");
        let ctx = DeviceContext::resolve(&dev, false).unwrap();
        assert_eq!(ctx.input_count(), Some(2));
    }

    #[test]
    fn input_count_unknown_model_no_schema_is_err() {
        let dev = mock_with_name("Unknown Device-00031337");
        let err = DeviceContext::resolve(&dev, false);
        assert!(err.is_err(), "unknown device with no schema should be Err");
    }
}
