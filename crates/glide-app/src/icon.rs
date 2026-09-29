//! The tray icon, drawn at runtime at the exact size Windows asks for, so it stays
//! sharp at any display scaling. The artwork lives in `icon_art.rs`.

use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use glide_app::icon_art::icon_pixels;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    On,
    Off,
}

/// Builds an `HICON` of `size` × `size` pixels. The caller owns it.
pub fn create(size: u32, look: Look) -> windows::core::Result<HICON> {
    let pixels = icon_pixels(size, look == Look::On);
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
    use glide_app::icon_art::{icon_file, icon_pixels};

    #[test]
    fn icon_has_transparent_corners_and_a_solid_middle() {
        for size in [16, 20, 24, 32, 48, 256] {
            let px = icon_pixels(size, true);
            let alpha = |x: u32, y: u32| px[(y * size + x) as usize] >> 24;
            assert_eq!(alpha(0, 0), 0, "size {size}");
            assert_eq!(alpha(size / 2, size / 2), 255, "size {size}");
        }
    }

    #[test]
    fn off_looks_grey() {
        let px = icon_pixels(32, false)[(16 * 32 + 2) as usize];
        let (r, g, b) = ((px >> 16) & 0xff, (px >> 8) & 0xff, px & 0xff);
        assert!(r.abs_diff(g) < 8 && g.abs_diff(b) < 8, "{r} {g} {b}");
    }

    #[test]
    fn ico_file_is_well_formed() {
        let sizes = [16, 32, 256];
        let ico = icon_file(&sizes);
        let u16_at = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(ico[i..i + 4].try_into().unwrap());
        assert_eq!((u16_at(0), u16_at(2), u16_at(4)), (0, 1, 3));
        let mut end = 6 + 16 * sizes.len();
        for (n, &size) in sizes.iter().enumerate() {
            let entry = 6 + 16 * n;
            assert_eq!(ico[entry], if size == 256 { 0 } else { size as u8 });
            let (len, offset) = (u32_at(entry + 8) as usize, u32_at(entry + 12) as usize);
            assert_eq!(offset, end, "images are packed in order");
            assert_eq!(u32_at(offset + 4), size, "BITMAPINFOHEADER width");
            end = offset + len;
        }
        assert_eq!(end, ico.len());
    }
}
