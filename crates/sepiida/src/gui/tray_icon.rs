//! The tray icon picture: a dot in the current accent colour, drawn in
//! code so no image file has to ship or be decoded.

use sepiida_core::Color;

/// Square size in pixels. Windows scales it down to the tray size (16 px at
/// 100 % scaling, 32 px at 200 %).
pub const ICON_SIZE: u32 = 64;
/// Until the first apply finishes.
pub const NEUTRAL: Color = Color::new(0x80, 0x80, 0x80);

/// Outer edge of the dot, as a fraction of the icon size.
const RADIUS: f32 = 0.44;
/// Width of the darker rim that keeps a pale accent visible on a light
/// taskbar, as a fraction of the icon size.
const RIM: f32 = 0.06;
/// How much darker the rim is than the fill.
const RIM_SHADE: f32 = 0.6;

/// RGBA pixels, row by row, `size × size`. Outside the dot is transparent;
/// the edge is anti-aliased.
pub fn dot_pixels(size: u32, fill: Color) -> Vec<u8> {
    let side = size as f32;
    let center = side / 2.0;
    let outer = side * RADIUS;
    let inner = outer - side * RIM;
    let shade = |c: u8| (f32::from(c) * RIM_SHADE).round() as u8;
    let rim = Color::new(shade(fill.r), shade(fill.g), shade(fill.b));

    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (outer - distance + 0.5).clamp(0.0, 1.0);
            let color = if distance < inner { fill } else { rim };
            pixels.extend_from_slice(&[
                color.r,
                color.g,
                color.b,
                (coverage * 255.0).round() as u8,
            ]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn draws_an_opaque_dot_on_a_transparent_square() {
        let fill = Color::new(200, 100, 50);
        let pixels = dot_pixels(ICON_SIZE, fill);
        assert_eq!(pixels.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert_eq!(pixel(&pixels, ICON_SIZE, 32, 32), [200, 100, 50, 255]);
        assert_eq!(pixel(&pixels, ICON_SIZE, 0, 0)[3], 0, "corner is clear");
        let rim = pixel(&pixels, ICON_SIZE, 32, 4);
        assert_eq!(rim[..3], [120, 60, 30], "rim is a darker shade");
        assert_eq!(rim[3], 255);
    }

    #[test]
    fn the_edge_is_anti_aliased() {
        let pixels = dot_pixels(ICON_SIZE, NEUTRAL);
        let row: Vec<u8> = (0..ICON_SIZE)
            .map(|x| pixel(&pixels, ICON_SIZE, x, ICON_SIZE / 2)[3])
            .collect();
        assert!(row.iter().any(|&a| a > 0 && a < 255), "{row:?}");
    }

    #[test]
    fn tiny_sizes_do_not_panic() {
        assert!(dot_pixels(0, NEUTRAL).is_empty());
        assert_eq!(dot_pixels(1, NEUTRAL).len(), 4);
    }
}
