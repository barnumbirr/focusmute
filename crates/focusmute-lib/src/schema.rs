//! Schema extraction — reads firmware devmap to discover per-model constants.
//!
//! The firmware schema is a base64-encoded, zlib-compressed JSON blob stored
//! across devmap pages. It contains LED counts, descriptor offsets, notify IDs,
//! and other model-specific data needed for safe multi-model support.
//!
//! ## Protocol
//!
//! 1. **INFO_DEVMAP** (SwRoot `0x000C0800`): returns `{ u16 unknown, u16 config_len }`
//!    after the 8-byte transact header. `config_len` (u16 LE at offset 10) is the
//!    byte length of the base64 content.
//! 2. **GET_DEVMAP** (SwRoot `0x000D0800`): read `ceil(config_len / 1024)` pages,
//!    each returning 8-byte header + up to 1024 bytes of base64 payload.
//! 3. **Decode**: strip trailing nulls → base64 decode → zlib decompress → JSON.
//!
//! Verified on Scarlett 2i2 4th Gen (fw 2.0.2417.0): config_len=5333, 6 pages,
//! decompresses to ~25KB JSON.

use std::io::Read as _;
use std::path::PathBuf;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::device::{DeviceError, Result, ScarlettDevice};
use crate::protocol::*;

/// Constants extracted from the firmware schema for a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConstants {
    pub product_name: String,
    /// kMAX_NUMBER_LEDS from enum maximum_array_sizes.
    pub max_leds: usize,
    /// kMAX_NUMBER_INPUTS from enum maximum_array_sizes.
    pub max_inputs: usize,
    /// kMAX_NUMBER_OUTPUTS from enum maximum_array_sizes.
    pub max_outputs: usize,
    /// LEDcolors array-shape[0] — number of gradient entries.
    pub gradient_count: usize,
    /// LEDcolors offset in descriptor.
    pub gradient_offset: u32,
    /// LEDcolors notify-device event ID.
    pub gradient_notify: u32,
    /// directLEDValues array-shape[0] — number of direct LED entries.
    pub direct_led_count: usize,
    /// directLEDValues offset in descriptor.
    pub direct_led_offset: u32,

    /// kNUMBER_METERING_SEGMENTS — total halo segments (e.g., 25 for 2i2 = 2×7 + 11).
    #[serde(default)]
    pub metering_segments: usize,

    /// Control names from physical-inputs[0].controls (e.g., ["air", "instrument", "phantom-power"]).
    #[serde(default)]
    pub input_controls: Vec<String>,

    /// APP_SPACE member names implying front-panel buttons (e.g., ["directMonitoring", "selectedInput"]).
    #[serde(default)]
    pub app_space_features: Vec<String>,

    /// Firmware version string at time of extraction (e.g., "2.0.2417.0").
    /// Used for cache invalidation when firmware is updated.
    #[serde(default)]
    pub firmware_version: String,
}

/// Read raw schema pages from device, concatenate payloads.
pub fn read_schema_raw(device: &impl ScarlettDevice) -> Result<Vec<u8>> {
    // Step 1: Get schema content length via INFO_DEVMAP.
    // Response payload (after 8-byte transact header): { u16 unknown, u16 config_len }
    let info_resp = device.transact(CMD_INFO_DEVMAP, &[], 12)?;
    if info_resp.len() < 12 {
        return Err(DeviceError::TransactFailed(format!(
            "INFO_DEVMAP response too short: {} bytes (expected >=12)",
            info_resp.len()
        )));
    }
    let total_size = u16::from_le_bytes(info_resp[10..12].try_into().unwrap_or_default()) as usize;
    if total_size == 0 {
        return Err(DeviceError::TransactFailed(
            "INFO_DEVMAP returned config_len 0".into(),
        ));
    }

    // Step 2: Read pages
    let page_count = total_size.div_ceil(DEVMAP_PAGE_SIZE);
    let mut raw = Vec::with_capacity(total_size);

    for page in 0..page_count {
        let payload = (page as u32).to_le_bytes();
        let resp = device.transact(CMD_GET_DEVMAP, &payload, DEVMAP_RESPONSE_SIZE)?;
        if resp.len() <= 8 {
            return Err(DeviceError::TransactFailed(format!(
                "GET_DEVMAP page {page} response too short: {} bytes",
                resp.len()
            )));
        }
        raw.extend_from_slice(&resp[8..]);
    }

    // Trim to exact total_size
    raw.truncate(total_size);
    Ok(raw)
}

/// Decode raw schema bytes into a JSON string.
///
/// Tries two formats:
/// 1. base64 → zlib → JSON (some firmware versions)
/// 2. raw zlib → JSON (observed on Scarlett 2i2 4th Gen firmware 2.x)
pub fn decode_schema(raw: &[u8]) -> crate::error::Result<String> {
    // Strip trailing null bytes — the devmap allocates more space than the
    // actual base64 content, padding the rest with zeros.
    let trimmed = &raw[..raw.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1)];

    // Guard against unreasonably large input (real schemas are ~34KB base64).
    const MAX_SCHEMA_BASE64: usize = 100_000;
    if trimmed.len() > MAX_SCHEMA_BASE64 {
        return Err(crate::FocusmuteError::Schema(format!(
            "schema data too large: {} bytes (max {MAX_SCHEMA_BASE64})",
            trimmed.len()
        )));
    }

    // Try base64 → zlib first
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed)
        && let Ok(json) = zlib_decompress_utf8(&decoded)
    {
        return Ok(json);
    }

    // Fall back to raw zlib (no base64 layer)
    if let Ok(json) = zlib_decompress_utf8(trimmed) {
        return Ok(json);
    }

    Err(crate::FocusmuteError::Schema(format!(
        "decode failed: not valid base64+zlib or raw zlib \
         ({} content bytes, first 8: {:02X?})",
        trimmed.len(),
        &trimmed[..trimmed.len().min(8)]
    )))
}

/// Maximum decompressed schema size (1 MB).
///
/// Real schemas are ~25 KB. This cap protects against corrupt or malicious
/// firmware data that could decompress to an unbounded size.
const MAX_SCHEMA_DECOMPRESSED: u64 = 1_048_576;

/// Zlib-decompress bytes and return as UTF-8 string.
fn zlib_decompress_utf8(data: &[u8]) -> std::result::Result<String, String> {
    let decoder = flate2::read::ZlibDecoder::new(data);
    let mut limited = decoder.take(MAX_SCHEMA_DECOMPRESSED);
    let mut json_bytes = Vec::new();
    limited
        .read_to_end(&mut json_bytes)
        .map_err(|e| format!("zlib decompress failed: {e}"))?;
    String::from_utf8(json_bytes).map_err(|e| format!("not valid UTF-8: {e}"))
}

/// Schema-specific error helper — wraps a string into `FocusmuteError::Schema`.
fn schema_err(msg: impl Into<String>) -> crate::FocusmuteError {
    crate::FocusmuteError::Schema(msg.into())
}

/// Parse JSON schema into SchemaConstants.
pub fn parse_schema(json: &str) -> crate::error::Result<SchemaConstants> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|e| schema_err(format!("JSON parse failed: {e}")))?;

    // Extract product name
    let product_name = root
        .pointer("/device-specification/product-name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| schema_err("missing device-specification.product-name"))?
        .to_string();

    // Extract enum constants from enums.maximum_array_sizes.enumerators
    let enumerators = root
        .pointer("/enums/maximum_array_sizes/enumerators")
        .ok_or_else(|| schema_err("missing enums.maximum_array_sizes.enumerators"))?;

    let max_leds = extract_enum_value(enumerators, "kMAX_NUMBER_LEDS")?;
    let max_inputs = extract_enum_value(enumerators, "kMAX_NUMBER_INPUTS")?;
    let max_outputs = extract_enum_value(enumerators, "kMAX_NUMBER_OUTPUTS")?;

    // Extract LEDcolors member
    let led_colors = root
        .pointer("/structs/APP_SPACE/members/LEDcolors")
        .ok_or_else(|| schema_err("missing structs.APP_SPACE.members.LEDcolors"))?;

    let gradient_count = led_colors
        .get("array-shape")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err("missing LEDcolors array-shape[0]"))?
        as usize;

    let gradient_offset = led_colors
        .get("offset")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err("missing LEDcolors offset"))? as u32;

    let gradient_notify = led_colors
        .get("notify-device")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err("missing LEDcolors notify-device"))?
        as u32;

    // Extract directLEDValues member
    let direct_leds = root
        .pointer("/structs/APP_SPACE/members/directLEDValues")
        .ok_or_else(|| schema_err("missing structs.APP_SPACE.members.directLEDValues"))?;

    let direct_led_count = direct_leds
        .get("array-shape")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err("missing directLEDValues array-shape[0]"))?
        as usize;

    let direct_led_offset = direct_leds
        .get("offset")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err("missing directLEDValues offset"))?
        as u32;

    // Extract kNUMBER_METERING_SEGMENTS (optional — default 0 if missing)
    let metering_segments = enumerators
        .get("kNUMBER_METERING_SEGMENTS")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    // Extract physical-inputs[0].controls keys (best-effort)
    let input_controls = root
        .pointer("/device-specification/physical-inputs")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|input| input.get("controls"))
        .and_then(|c| c.as_object())
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    // Check APP_SPACE members for directMonitoring and selectedInput
    let app_space_features = {
        let members = root
            .pointer("/structs/APP_SPACE/members")
            .and_then(|v| v.as_object());
        let mut features = Vec::new();
        if let Some(m) = members {
            for key in ["directMonitoring", "selectedInput"] {
                if m.contains_key(key) {
                    features.push(key.to_string());
                }
            }
        }
        features
    };

    Ok(SchemaConstants {
        product_name,
        max_leds,
        max_inputs,
        max_outputs,
        gradient_count,
        gradient_offset,
        gradient_notify,
        direct_led_count,
        direct_led_offset,
        metering_segments,
        input_controls,
        app_space_features,
        firmware_version: String::new(),
    })
}

/// Full pipeline: read from device → decode → parse.
pub fn extract_schema(device: &impl ScarlettDevice) -> crate::error::Result<SchemaConstants> {
    let raw = read_schema_raw(device)?;
    let json = decode_schema(&raw)?;
    parse_schema(&json)
}

// ── Schema caching ──

/// Cache path: config_dir/Focusmute/schema_cache.json
pub fn cache_path() -> Option<PathBuf> {
    crate::config::Config::dir().map(|d| d.join("schema_cache.json"))
}

/// Save SchemaConstants to cache file.
pub fn save_cache(constants: &SchemaConstants) -> std::io::Result<()> {
    let path = cache_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No config directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(constants).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

/// Load SchemaConstants from cache file, if it exists and matches the model name + firmware.
///
/// `model_name` should be the cleaned model name (e.g. "Scarlett 2i2 4th Gen") —
/// callers should use `DeviceInfo::model()` to strip the serial suffix.
pub fn load_cache(model_name: &str, firmware_version: &str) -> Option<SchemaConstants> {
    let path = cache_path()?;
    load_cache_from(&path, model_name, firmware_version)
}

/// Load SchemaConstants from a specific path. Testable without relying on platform config dirs.
pub fn load_cache_from(
    path: &std::path::Path,
    model_name: &str,
    firmware_version: &str,
) -> Option<SchemaConstants> {
    let data = std::fs::read_to_string(path).ok()?;
    let cached: SchemaConstants = serde_json::from_str(&data).ok()?;
    if !cached.product_name.eq_ignore_ascii_case(model_name) {
        return None;
    }
    // Firmware version must match (empty cached version = old cache, force re-extraction)
    if cached.firmware_version.is_empty() || cached.firmware_version != firmware_version {
        return None;
    }
    Some(cached)
}

/// Extract schema from device, using cache when available.
pub fn extract_or_cached(device: &impl ScarlettDevice) -> crate::error::Result<SchemaConstants> {
    let info = device.info();
    let fw = info.firmware.to_string();
    if let Some(cached) = load_cache(info.model(), &fw) {
        return Ok(cached);
    }
    let mut constants = extract_schema(device)?;
    constants.firmware_version = fw;
    let _ = save_cache(&constants);
    Ok(constants)
}

// ── Helpers ──

fn extract_enum_value(enumerators: &serde_json::Value, key: &str) -> crate::error::Result<usize> {
    enumerators
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| schema_err(format!("missing enum value: {key}")))
        .map(|v| v as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::mock::MockDevice;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    /// Minimal valid schema JSON for testing.
    fn test_schema_json() -> String {
        serde_json::json!({
            "device-specification": {
                "product-name": "Scarlett 2i2 4th Gen"
            },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string()
    }

    /// Encode JSON → zlib → base64 (reverse of decode_schema).
    fn encode_schema(json: &str) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        base64::engine::general_purpose::STANDARD
            .encode(&compressed)
            .into_bytes()
    }

    #[test]
    fn decode_schema_roundtrip() {
        let original = test_schema_json();
        let raw = encode_schema(&original);
        let decoded = decode_schema(&raw).unwrap();
        // Parse both to compare structurally (formatting may differ)
        let orig_val: serde_json::Value = serde_json::from_str(&original).unwrap();
        let decoded_val: serde_json::Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(orig_val, decoded_val);
    }

    #[test]
    fn decode_schema_invalid_base64() {
        let result = decode_schema(b"not-valid-base64!!!");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base64+zlib"));
    }

    #[test]
    fn decode_schema_invalid_zlib() {
        // Valid base64 but not valid zlib
        let raw = base64::engine::general_purpose::STANDARD
            .encode(b"not zlib data")
            .into_bytes();
        let result = decode_schema(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("zlib"));
    }

    #[test]
    fn parse_schema_valid() {
        let json = test_schema_json();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.product_name, "Scarlett 2i2 4th Gen");
        assert_eq!(constants.max_leds, 40);
        assert_eq!(constants.max_inputs, 2);
        assert_eq!(constants.max_outputs, 2);
        assert_eq!(constants.gradient_count, 11);
        assert_eq!(constants.gradient_offset, 384);
        assert_eq!(constants.gradient_notify, 9);
        assert_eq!(constants.direct_led_count, 40);
        assert_eq!(constants.direct_led_offset, 92);
    }

    #[test]
    fn parse_schema_missing_product_name() {
        let json = r#"{"enums":{},"structs":{}}"#;
        let result = parse_schema(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("product-name"));
    }

    #[test]
    fn parse_schema_missing_led_colors() {
        let json = serde_json::json!({
            "device-specification": { "product-name": "Test" },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {}
                }
            }
        })
        .to_string();
        let result = parse_schema(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("LEDcolors"));
    }

    #[test]
    fn parse_schema_missing_enum_values() {
        let json = serde_json::json!({
            "device-specification": { "product-name": "Test" },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {}
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string();
        let result = parse_schema(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kMAX_NUMBER_LEDS"));
    }

    #[test]
    fn extract_schema_end_to_end_mock() {
        let raw = encode_schema(&test_schema_json());
        let total_size = raw.len();

        // Build mock INFO_DEVMAP response: 8-byte header + { u16 unknown, u16 config_len }
        let mut info_resp = vec![0u8; 8];
        info_resp.extend_from_slice(&0u16.to_le_bytes()); // unknown
        info_resp.extend_from_slice(&(total_size as u16).to_le_bytes()); // config_len

        // Build mock GET_DEVMAP page responses
        let page_count = total_size.div_ceil(DEVMAP_PAGE_SIZE);

        let dev = MockDevice::new();
        dev.add_transact_response(CMD_INFO_DEVMAP, info_resp);

        for page in 0..page_count {
            let start = page * DEVMAP_PAGE_SIZE;
            let end = (start + DEVMAP_PAGE_SIZE).min(total_size);
            let mut page_resp = vec![0u8; 8]; // 8-byte header
            page_resp.extend_from_slice(&raw[start..end]);
            // Pad to full page size if needed
            page_resp.resize(DEVMAP_RESPONSE_SIZE, 0);
            dev.add_transact_response(CMD_GET_DEVMAP, page_resp);
        }

        let constants = extract_schema(&dev).unwrap();
        assert_eq!(constants.product_name, "Scarlett 2i2 4th Gen");
        assert_eq!(constants.gradient_count, 11);
        assert_eq!(constants.direct_led_count, 40);
    }

    #[test]
    fn extract_schema_device_error_propagates() {
        let dev = MockDevice::new();
        // No handlers registered — should fail
        let result = extract_schema(&dev);
        assert!(result.is_err());
        // Should propagate as Device error (from read_schema_raw → transact)
        assert!(matches!(
            result.unwrap_err(),
            crate::FocusmuteError::Device(_)
        ));
    }

    #[test]
    fn schema_constants_serde_roundtrip() {
        let constants = SchemaConstants {
            product_name: "Test Device".into(),
            max_leds: 56,
            max_inputs: 4,
            max_outputs: 4,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 56,
            direct_led_offset: 92,
            metering_segments: 39,
            input_controls: vec!["air".into(), "instrument".into()],
            app_space_features: vec!["directMonitoring".into()],
            firmware_version: "2.0.2417.0".into(),
        };
        let json = serde_json::to_string(&constants).unwrap();
        let restored: SchemaConstants = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.product_name, constants.product_name);
        assert_eq!(restored.gradient_count, constants.gradient_count);
        assert_eq!(restored.direct_led_count, constants.direct_led_count);
        assert_eq!(restored.metering_segments, 39);
        assert_eq!(restored.input_controls, vec!["air", "instrument"]);
        assert_eq!(restored.app_space_features, vec!["directMonitoring"]);
        assert_eq!(restored.firmware_version, "2.0.2417.0");
    }

    #[test]
    fn schema_backward_compat_deserialize() {
        // JSON without the new fields — should deserialize with defaults
        let json = r#"{
            "product_name": "Old Device",
            "max_leds": 40,
            "max_inputs": 2,
            "max_outputs": 2,
            "gradient_count": 11,
            "gradient_offset": 384,
            "gradient_notify": 9,
            "direct_led_count": 40,
            "direct_led_offset": 92
        }"#;
        let restored: SchemaConstants = serde_json::from_str(json).unwrap();
        assert_eq!(restored.product_name, "Old Device");
        assert_eq!(restored.metering_segments, 0);
        assert!(restored.input_controls.is_empty());
        assert!(restored.app_space_features.is_empty());
    }

    #[test]
    fn parse_schema_extracts_metering_segments() {
        let json = serde_json::json!({
            "device-specification": {
                "product-name": "Scarlett 2i2 4th Gen",
                "physical-inputs": []
            },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2,
                        "kNUMBER_METERING_SEGMENTS": 25
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        },
                        "directMonitoring": {
                            "type": "uint8",
                            "offset": 330
                        },
                        "selectedInput": {
                            "type": "uint8",
                            "offset": 331
                        }
                    }
                }
            }
        })
        .to_string();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.metering_segments, 25);
        assert!(
            constants
                .app_space_features
                .contains(&"directMonitoring".to_string())
        );
        assert!(
            constants
                .app_space_features
                .contains(&"selectedInput".to_string())
        );
    }

    #[test]
    fn parse_schema_extracts_input_controls() {
        let json = serde_json::json!({
            "device-specification": {
                "product-name": "Scarlett 2i2 4th Gen",
                "physical-inputs": [
                    {
                        "name": "Analogue 1",
                        "controls": {
                            "air": {"struct": "APP_SPACE", "member": "inputAir"},
                            "instrument": {"struct": "APP_SPACE", "member": "instInput"},
                            "phantom-power": {"struct": "APP_SPACE", "member": "enablePhantomPower"},
                            "clip-safe": {"struct": "APP_SPACE", "member": "clipSafe"},
                            "auto-gain": {"struct": "APP_SPACE", "member": "autogainInProgress"}
                        }
                    }
                ]
            },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.input_controls.len(), 5);
        assert!(constants.input_controls.contains(&"air".to_string()));
        assert!(constants.input_controls.contains(&"instrument".to_string()));
        assert!(
            constants
                .input_controls
                .contains(&"phantom-power".to_string())
        );
        assert!(constants.input_controls.contains(&"clip-safe".to_string()));
        assert!(constants.input_controls.contains(&"auto-gain".to_string()));
    }

    #[test]
    fn parse_schema_metering_segments_defaults_zero() {
        // Schema without kNUMBER_METERING_SEGMENTS
        let json = test_schema_json();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.metering_segments, 0);
    }

    // ── Firmware version cache ──

    #[test]
    fn backward_compat_old_cache_without_firmware_version() {
        // Old cache without firmware_version — should deserialize with empty string
        let json = r#"{
            "product_name": "Old Device",
            "max_leds": 40,
            "max_inputs": 2,
            "max_outputs": 2,
            "gradient_count": 11,
            "gradient_offset": 384,
            "gradient_notify": 9,
            "direct_led_count": 40,
            "direct_led_offset": 92
        }"#;
        let restored: SchemaConstants = serde_json::from_str(json).unwrap();
        assert!(restored.firmware_version.is_empty());
    }

    #[test]
    fn firmware_version_serde_roundtrip() {
        let constants = SchemaConstants {
            product_name: "Test Device".into(),
            max_leds: 40,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 40,
            direct_led_offset: 92,
            metering_segments: 0,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: "2.0.2417.0".into(),
        };
        let json = serde_json::to_string(&constants).unwrap();
        let restored: SchemaConstants = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.firmware_version, "2.0.2417.0");
    }

    /// Helper: write a SchemaConstants to a temp file and return the path.
    fn write_test_cache(name: &str, constants: &SchemaConstants) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("focusmute_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("schema_cache.json");
        let json = serde_json::to_string_pretty(constants).unwrap();
        std::fs::write(&path, &json).unwrap();
        path
    }

    fn test_constants(fw: &str) -> SchemaConstants {
        SchemaConstants {
            product_name: "Scarlett 2i2 4th Gen".into(),
            max_leds: 40,
            max_inputs: 2,
            max_outputs: 2,
            gradient_count: 11,
            gradient_offset: 384,
            gradient_notify: 9,
            direct_led_count: 40,
            direct_led_offset: 92,
            metering_segments: 25,
            input_controls: vec![],
            app_space_features: vec![],
            firmware_version: fw.into(),
        }
    }

    #[test]
    fn load_cache_from_matching() {
        let path = write_test_cache("cache_match", &test_constants("2.0.2417.0"));
        let result = load_cache_from(&path, "Scarlett 2i2 4th Gen", "2.0.2417.0");
        assert!(result.is_some());
        assert_eq!(result.unwrap().gradient_count, 11);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_cache_from_mismatched_fw() {
        let path = write_test_cache("cache_fw_mismatch", &test_constants("1.0.0.0"));
        let result = load_cache_from(&path, "Scarlett 2i2 4th Gen", "2.0.2417.0");
        assert!(result.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_cache_from_empty_fw() {
        let path = write_test_cache("cache_empty_fw", &test_constants(""));
        let result = load_cache_from(&path, "Scarlett 2i2 4th Gen", "2.0.2417.0");
        assert!(result.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_cache_from_mismatched_model() {
        let path = write_test_cache("cache_model_mismatch", &test_constants("2.0.2417.0"));
        let result = load_cache_from(&path, "Scarlett 4i4 4th Gen", "2.0.2417.0");
        assert!(result.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_cache_from_nonexistent_file() {
        let path = PathBuf::from("/tmp/focusmute_nonexistent_cache.json");
        let result = load_cache_from(&path, "Scarlett 2i2 4th Gen", "2.0.2417.0");
        assert!(result.is_none());
    }

    // ── Edge case tests (10A) ──

    #[test]
    fn decode_schema_garbage_bytes() {
        // Random bytes that are neither valid base64+zlib nor raw zlib.
        let raw = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8];
        let result = decode_schema(&raw);
        assert!(result.is_err(), "garbage bytes should fail");
        assert!(
            result.unwrap_err().to_string().contains("base64+zlib"),
            "error should mention both decode paths"
        );
    }

    #[test]
    fn parse_schema_gradient_count_zero() {
        let json = serde_json::json!({
            "device-specification": { "product-name": "Test Zero Gradient" },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [0],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.gradient_count, 0);
        // gradient_count=0 is valid per the schema — downstream code handles it
    }

    #[test]
    fn parse_schema_direct_led_offset_zero() {
        // An offset of 0 would overlap with the enable_direct_led byte — a
        // realistic corrupt/misconfigured schema edge case.
        let json = serde_json::json!({
            "device-specification": { "product-name": "Test Zero Offset" },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 40,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 0,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string();
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.direct_led_offset, 0);
    }

    #[test]
    fn parse_schema_absurdly_large_max_leds() {
        let json = serde_json::json!({
            "device-specification": { "product-name": "Test Huge LEDs" },
            "enums": {
                "maximum_array_sizes": {
                    "enumerators": {
                        "kMAX_NUMBER_LEDS": 999999999,
                        "kMAX_NUMBER_INPUTS": 2,
                        "kMAX_NUMBER_OUTPUTS": 2
                    }
                }
            },
            "structs": {
                "APP_SPACE": {
                    "members": {
                        "LEDcolors": {
                            "offset": 384,
                            "array-shape": [11],
                            "notify-device": 9
                        },
                        "directLEDValues": {
                            "offset": 92,
                            "array-shape": [40]
                        }
                    }
                }
            }
        })
        .to_string();
        // Should parse without panic — the value is just stored, not allocated
        let constants = parse_schema(&json).unwrap();
        assert_eq!(constants.max_leds, 999999999);
    }

    #[test]
    fn decode_schema_rejects_oversized_input() {
        let large = vec![b'A'; 100_001];
        let result = decode_schema(&large);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
