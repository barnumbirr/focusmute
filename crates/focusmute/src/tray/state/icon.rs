//! Tray icon loading and caching.

use tray_icon::Icon;

// Embedded tray icons (multi-size ICO files).
const ICON_LIVE_ICO: &[u8] = include_bytes!("../../../assets/icon-live.ico");
const ICON_MUTED_ICO: &[u8] = include_bytes!("../../../assets/icon-muted.ico");

/// Target size when extracting tray icons from ICO files.  32 px is a good
/// compromise: Windows tray icons range from 16 px (100 % DPI) to 32 px
/// (200 % DPI), so the worst-case downscale is only 2:1.
const TRAY_ICON_SIZE: u8 = 32;

// ── Icon loading (decoded once, cloned on use) ──

/// RGBA pixel data cached for cheap cloning into `Icon`.
struct CachedIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

impl CachedIcon {
    fn decode(ico_data: &[u8]) -> Self {
        let img = decode_ico_entry(ico_data, TRAY_ICON_SIZE)
            .expect("Failed to decode embedded icon")
            .into_rgba8();
        let (w, h) = img.dimensions();
        Self {
            rgba: img.into_raw(),
            width: w,
            height: h,
        }
    }

    fn to_icon(&self) -> Icon {
        Icon::from_rgba(self.rgba.clone(), self.width, self.height).expect("icon creation")
    }
}

/// Extract a specific size from a multi-size ICO file.
///
/// Parses the ICO directory to find the entry closest to `target_size`,
/// then decodes that entry's image data.  `image::load_from_memory` always
/// returns the largest entry (256 px), which loses thin details like the
/// crossbar when Windows downscales it to tray size (16–24 px).
fn decode_ico_entry(
    ico_data: &[u8],
    target_size: u8,
) -> Result<image::DynamicImage, image::ImageError> {
    // ICO header: 2 reserved + 2 type + 2 count = 6 bytes
    // Each directory entry: 16 bytes (width, height, ..., 4-byte offset, 4-byte size)
    if ico_data.len() < 6 {
        return image::load_from_memory(ico_data);
    }
    let count = u16::from_le_bytes([ico_data[4], ico_data[5]]) as usize;

    let mut best_idx = 0;
    let mut best_diff = u16::MAX;
    for i in 0..count {
        let entry_offset = 6 + i * 16;
        if entry_offset + 16 > ico_data.len() {
            break;
        }
        // Width byte: 0 means 256
        let w = if ico_data[entry_offset] == 0 {
            256u16
        } else {
            ico_data[entry_offset] as u16
        };
        let diff = (w as i32 - target_size as i32).unsigned_abs() as u16;
        if diff < best_diff {
            best_diff = diff;
            best_idx = i;
        }
    }

    // Read offset and size of the chosen entry's image data
    let entry = 6 + best_idx * 16;
    let data_size = u32::from_le_bytes([
        ico_data[entry + 8],
        ico_data[entry + 9],
        ico_data[entry + 10],
        ico_data[entry + 11],
    ]) as usize;
    let data_offset = u32::from_le_bytes([
        ico_data[entry + 12],
        ico_data[entry + 13],
        ico_data[entry + 14],
        ico_data[entry + 15],
    ]) as usize;

    if data_offset + data_size <= ico_data.len() {
        let entry_data = &ico_data[data_offset..data_offset + data_size];
        // Individual entries are typically PNG or BMP; image crate handles both.
        image::load_from_memory(entry_data)
    } else {
        image::load_from_memory(ico_data)
    }
}

pub fn icon_live() -> Icon {
    use std::sync::OnceLock;
    static CACHE: OnceLock<CachedIcon> = OnceLock::new();
    CACHE
        .get_or_init(|| CachedIcon::decode(ICON_LIVE_ICO))
        .to_icon()
}

pub fn icon_muted() -> Icon {
    use std::sync::OnceLock;
    static CACHE: OnceLock<CachedIcon> = OnceLock::new();
    CACHE
        .get_or_init(|| CachedIcon::decode(ICON_MUTED_ICO))
        .to_icon()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal synthetic ICO file with given entries.
    /// Each entry is `(width_byte, png_data)`. Width byte 0 means 256px.
    fn build_synthetic_ico(entries: &[(u8, &[u8])]) -> Vec<u8> {
        let count = entries.len() as u16;
        let header_size = 6 + entries.len() * 16;
        let mut ico = Vec::new();

        // ICO header: reserved(2) + type(2) + count(2)
        ico.extend_from_slice(&[0, 0]); // reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (ICO)
        ico.extend_from_slice(&count.to_le_bytes());

        // Calculate data offsets
        let mut data_offset = header_size;
        for (width, png_data) in entries {
            let size = png_data.len() as u32;
            // Directory entry: width, height, color_count, reserved, planes(2), bpp(2), size(4), offset(4)
            ico.push(*width); // width
            ico.push(*width); // height (same as width for simplicity)
            ico.push(0); // color count
            ico.push(0); // reserved
            ico.extend_from_slice(&1u16.to_le_bytes()); // planes
            ico.extend_from_slice(&32u16.to_le_bytes()); // bpp
            ico.extend_from_slice(&size.to_le_bytes()); // data size
            ico.extend_from_slice(&(data_offset as u32).to_le_bytes()); // data offset
            data_offset += png_data.len();
        }

        // Append image data
        for (_, png_data) in entries {
            ico.extend_from_slice(png_data);
        }

        ico
    }

    /// Create a minimal valid PNG with the given dimensions.
    fn minimal_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(width, height, Rgba([255u8, 0, 0, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn decode_ico_entry_selects_closest_size() {
        let png_16 = minimal_png(16, 16);
        let png_32 = minimal_png(32, 32);
        let ico = build_synthetic_ico(&[(16, &png_16), (32, &png_32)]);

        // Request 32px → should select the 32px entry
        let img = decode_ico_entry(&ico, 32).unwrap();
        assert_eq!(img.width(), 32);
        assert_eq!(img.height(), 32);

        // Request 16px → should select the 16px entry
        let img = decode_ico_entry(&ico, 16).unwrap();
        assert_eq!(img.width(), 16);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn decode_ico_entry_fallback_on_out_of_bounds() {
        let png_32 = minimal_png(32, 32);
        let mut ico = build_synthetic_ico(&[(32, &png_32)]);

        // Corrupt the data offset to point past EOF
        let offset_pos = 6 + 12; // first entry's offset field at byte 18
        let bad_offset = (ico.len() + 1000) as u32;
        ico[offset_pos..offset_pos + 4].copy_from_slice(&bad_offset.to_le_bytes());

        // Should fall back to image::load_from_memory on the whole ICO
        // This may fail to decode (corrupted), but it should NOT panic
        let result = decode_ico_entry(&ico, 32);
        // We just verify no panic — result may be Ok or Err depending on
        // whether image crate can make sense of the corrupted ICO
        let _ = result;
    }

    #[test]
    fn embedded_icons_decode_at_32px() {
        let live = decode_ico_entry(ICON_LIVE_ICO, TRAY_ICON_SIZE).unwrap();
        assert_eq!(live.width(), 32);
        assert_eq!(live.height(), 32);

        let muted = decode_ico_entry(ICON_MUTED_ICO, TRAY_ICON_SIZE).unwrap();
        assert_eq!(muted.width(), 32);
        assert_eq!(muted.height(), 32);
    }
}
