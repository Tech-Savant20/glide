// Glide's icon artwork, as code: a rounded tile holding the outline of a mouse
// with its wheel. Drawn at any size, so every size is sharp.
//
// This file has no dependencies because build.rs also `include!`s it, to write
// the .ico that is embedded in glide.exe.

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

/// Straight (non-premultiplied) ARGB pixels, top row first. `on` draws the blue
/// tile; otherwise it's grey, for "off" and "paused".
pub fn icon_pixels(size: u32, on: bool) -> Vec<u32> {
    let s = size as f64;
    let tile = if on {
        [0x2f, 0x6f, 0xdf]
    } else {
        [0x8e, 0x8e, 0x93]
    };
    let c = s / 2.0;
    // Keep the outline at least about a pixel wide at 16 px.
    let stroke = (s * 0.075).max(1.3);
    let mut out = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
            let tile_a = coverage(rounded_rect(x, y, c, c, c - 0.5, c - 0.5, s * 0.24));
            // The mouse: a white outline of a tall rounded body.
            let body = rounded_rect(x, y, c, s * 0.52, s * 0.2, s * 0.3, s * 0.2);
            let outline = coverage(body.abs() - stroke / 2.0);
            // Its wheel: a short white capsule near the top.
            let wheel = coverage(rounded_rect(
                x,
                y,
                c,
                s * 0.37,
                stroke / 2.0,
                s * 0.075,
                stroke / 2.0,
            ));
            let white = outline.max(wheel);
            let rgb: [f64; 3] =
                std::array::from_fn(|i| tile[i] as f64 * (1.0 - white) + 255.0 * white);
            let a = (tile_a * 255.0).round() as u32;
            let [r, g, b] = rgb.map(|v| v.round() as u32);
            out.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }
    out
}

/// An .ico file holding the "on" icon at each of `sizes` (each at most 256), as
/// 32-bit bitmaps with alpha. Used by build.rs for the icon in glide.exe.
#[allow(dead_code)]
pub fn icon_file(sizes: &[u32]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = Vec::new();
    for &size in sizes {
        let pixels = icon_pixels(size, true);
        let mask_row = size.div_ceil(32) * 4; // 1 bit per pixel, rows padded to 4 bytes
        let mut img = Vec::new();
        // BITMAPINFOHEADER; the height counts the colour and mask halves.
        img.extend_from_slice(&40u32.to_le_bytes());
        img.extend_from_slice(&(size as i32).to_le_bytes());
        img.extend_from_slice(&(size as i32 * 2).to_le_bytes());
        img.extend_from_slice(&1u16.to_le_bytes());
        img.extend_from_slice(&32u16.to_le_bytes());
        img.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        img.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
        img.extend_from_slice(&[0; 16]);
        // Colour rows, bottom row first.
        for row in (0..size).rev() {
            for col in 0..size {
                img.extend_from_slice(&pixels[(row * size + col) as usize].to_le_bytes());
            }
        }
        // An all-zero AND mask: the alpha channel does the masking.
        img.resize(img.len() + (mask_row * size) as usize, 0);
        images.push(img);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, img) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 }; // 0 means 256
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}
