//! The tray icon, drawn at runtime at the exact size Windows asks for, so it stays
//! sharp at any display scaling without shipping a set of .ico files.
//!
//! The glyph is a rounded tile holding a mouse wheel with a soft trail below it.

use windows::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    On,
    Off,
}

/// Coverage (0..1) of a pixel centred at `d` from an edge, for anti-aliasing.
fn coverage(d: f64) -> f64 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// Signed distance from `(x, y)` to a rounded rectangle centred at `(cx, cy)`.
fn rounded_rect(x: f64, y: f64, cx: f64, cy: f64, half_w: f64, half_h: f64, r: f64) -> f64 {
    let qx = (x - cx).abs() - (half_w - r);
    let qy = (y - cy).abs() - (half_h - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Straight (non-premultiplied) BGRA pixels, top row first.
pub fn pixels(size: u32, look: Look) -> Vec<u32> {
    let s = size as f64;
    let (tile, trail) = match look {
        Look::On => ([0x2f, 0x6f, 0xdf], 0.55),
        Look::Off => ([0x8e, 0x8e, 0x93], 0.0),
    };
    let mut out = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
            let c = s / 2.0;
            let tile_a = coverage(rounded_rect(x, y, c, c, s / 2.0 - 0.5, s / 2.0 - 0.5, s * 0.24));
            // The wheel: a white capsule, a little above centre.
            let wheel = coverage(rounded_rect(x, y, c, s * 0.42, s * 0.11, s * 0.2, s * 0.11));
            // The trail: a fainter capsule below it, suggesting motion.
            let trail_a = trail
                * coverage(rounded_rect(x, y, c, s * 0.76, s * 0.07, s * 0.07, s * 0.07));
            let white = wheel.max(trail_a);
            let rgb: [f64; 3] =
                std::array::from_fn(|i| tile[i] as f64 * (1.0 - white) + 255.0 * white);
            let a = (tile_a * 255.0).round() as u32;
            let [r, g, b] = rgb.map(|v| v.round() as u32);
            out.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }
    out
}

/// Builds an `HICON` of `size` × `size` pixels. The caller owns it.
pub fn create(size: u32, look: Look) -> windows::core::Result<HICON> {
    let pixels = pixels(size, look);
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color: HBITMAP = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        // With a 32-bit colour bitmap the alpha channel does the masking; the mask
        // still has to exist.
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            hbmMask: mask,
            hbmColor: color,
            ..Default::default()
        });
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_transparent_corners_and_a_solid_middle() {
        for size in [16, 20, 24, 32, 48] {
            let px = pixels(size, Look::On);
            let alpha = |x: u32, y: u32| px[(y * size + x) as usize] >> 24;
            assert_eq!(alpha(0, 0), 0, "size {size}");
            assert_eq!(alpha(size / 2, size / 2), 255, "size {size}");
        }
    }

    #[test]
    fn off_looks_grey() {
        let px = pixels(32, Look::Off)[(16 * 32 + 2) as usize];
        let (r, g, b) = ((px >> 16) & 0xff, (px >> 8) & 0xff, px & 0xff);
        assert!(r.abs_diff(g) < 8 && g.abs_diff(b) < 8, "{r} {g} {b}");
    }
}
