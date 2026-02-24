//! Color parsing and formatting for Focusrite LED control.
//!
//! Colors use the device format `0xRRGGBB00` (RGB shifted left 8 bits).

/// Parse a color string into the device format `0xRRGGBB00`.
///
/// Accepts:
/// - Hex: `"#FF0000"`, `"FF0000"`, `"#ff0000"`
/// - Named: `"red"`, `"green"`, `"blue"`, `"white"`, `"orange"`, `"yellow"`, `"purple"`, `"cyan"`
pub fn parse_color(s: &str) -> crate::error::Result<u32> {
    let s = s.trim();

    // Named colors
    match s.to_lowercase().as_str() {
        "red" => return Ok(0xFF00_0000),
        "green" => return Ok(0x00FF_0000),
        "blue" => return Ok(0x0000_FF00),
        "white" => return Ok(0xFFFF_FF00),
        "orange" => return Ok(0xFF80_0000),
        "yellow" => return Ok(0xFFFF_0000),
        "purple" => return Ok(0x8000_FF00),
        "cyan" => return Ok(0x00FF_FF00),
        "off" | "black" => return Ok(0x0000_0000),
        _ => {}
    }

    // Hex color
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return Err(crate::FocusmuteError::Color(format!(
            "Invalid color: {s} (use #RRGGBB or a color name)"
        )));
    }
    let val = u32::from_str_radix(hex, 16)
        .map_err(|_| crate::FocusmuteError::Color(format!("Invalid hex color: {s}")))?;
    Ok(val << 8) // shift to 0xRRGGBB00
}

/// Format a device color value as `#RRGGBB`.
pub fn format_color(val: u32) -> String {
    let r = (val >> 24) & 0xFF;
    let g = (val >> 16) & 0xFF;
    let b = (val >> 8) & 0xFF;
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_color ──

    #[test]
    fn parse_named_red() {
        assert_eq!(parse_color("red").unwrap(), 0xFF00_0000);
    }

    #[test]
    fn parse_named_green() {
        assert_eq!(parse_color("green").unwrap(), 0x00FF_0000);
    }

    #[test]
    fn parse_named_blue() {
        assert_eq!(parse_color("blue").unwrap(), 0x0000_FF00);
    }

    #[test]
    fn parse_named_white() {
        assert_eq!(parse_color("white").unwrap(), 0xFFFF_FF00);
    }

    #[test]
    fn parse_named_off() {
        assert_eq!(parse_color("off").unwrap(), 0x0000_0000);
        assert_eq!(parse_color("black").unwrap(), 0x0000_0000);
    }

    #[test]
    fn parse_named_case_insensitive() {
        assert_eq!(parse_color("RED").unwrap(), 0xFF00_0000);
        assert_eq!(parse_color("Red").unwrap(), 0xFF00_0000);
        assert_eq!(parse_color("  red  ").unwrap(), 0xFF00_0000);
    }

    #[test]
    fn parse_hex_with_hash() {
        assert_eq!(parse_color("#FF0000").unwrap(), 0xFF00_0000);
        assert_eq!(parse_color("#00FF00").unwrap(), 0x00FF_0000);
        assert_eq!(parse_color("#0000FF").unwrap(), 0x0000_FF00);
    }

    #[test]
    fn parse_hex_without_hash() {
        assert_eq!(parse_color("FF0000").unwrap(), 0xFF00_0000);
        assert_eq!(parse_color("ABCDEF").unwrap(), 0xABCD_EF00);
    }

    #[test]
    fn parse_hex_lowercase() {
        assert_eq!(parse_color("#ff8000").unwrap(), 0xFF80_0000);
        assert_eq!(parse_color("abcdef").unwrap(), 0xABCD_EF00);
    }

    #[test]
    fn parse_hex_shifts_left_8() {
        // Core property: 0xRRGGBB → 0xRRGGBB00
        assert_eq!(parse_color("#123456").unwrap(), 0x1234_5600);
    }

    #[test]
    fn parse_invalid_short() {
        assert!(parse_color("#FFF").is_err());
    }

    #[test]
    fn parse_invalid_long() {
        assert!(parse_color("#FF000000").is_err());
    }

    #[test]
    fn parse_invalid_name() {
        assert!(parse_color("chartreuse").is_err());
    }

    #[test]
    fn parse_invalid_hex_chars() {
        assert!(parse_color("#GGHHII").is_err());
    }

    // ── format_color ──

    #[test]
    fn format_red() {
        assert_eq!(format_color(0xFF00_0000), "#FF0000");
    }

    #[test]
    fn format_green() {
        assert_eq!(format_color(0x00FF_0000), "#00FF00");
    }

    #[test]
    fn format_blue() {
        assert_eq!(format_color(0x0000_FF00), "#0000FF");
    }

    #[test]
    fn format_white() {
        assert_eq!(format_color(0xFFFF_FF00), "#FFFFFF");
    }

    #[test]
    fn format_black() {
        assert_eq!(format_color(0x0000_0000), "#000000");
    }

    #[test]
    fn format_ignores_low_byte() {
        // The low byte (alpha/padding) should not affect output
        assert_eq!(format_color(0xFF0000FF), "#FF0000");
    }

    // ── round-trip ──

    #[test]
    fn parse_format_roundtrip() {
        for name in &[
            "red", "green", "blue", "white", "orange", "yellow", "purple", "cyan",
        ] {
            let val = parse_color(name).unwrap();
            let hex = format_color(val);
            let val2 = parse_color(&hex).unwrap();
            assert_eq!(val, val2, "round-trip failed for {name}");
        }
    }

    #[test]
    fn parse_format_roundtrip_hex() {
        let val = parse_color("#AB12CD").unwrap();
        assert_eq!(format_color(val), "#AB12CD");
        assert_eq!(parse_color("#AB12CD").unwrap(), val);
    }
}
