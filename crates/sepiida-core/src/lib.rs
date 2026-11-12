//! Platform-independent colour engine: wallpaper image in, palette out.

use std::{fmt, path::Path};

use image::{DynamicImage, imageops::FilterType};
use material_colors::{color::Rgb, image::extract_color, scheme::Scheme, theme::ThemeBuilder};

/// Quantization time grows with pixel count and the result barely changes
/// above this size, so every image is shrunk to fit this box first.
const SAMPLE_SIZE: u32 = 128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to read image")]
    Image(#[from] image::ImageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Self {
        Self {
            r: c.red,
            g: c.green,
            b: c.blue,
        }
    }
}

/// The colours applied to the shell for one mode (light or dark).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tones {
    pub accent: Color,
    pub on_accent: Color,
    pub surface: Color,
    pub on_surface: Color,
}

impl From<&Scheme> for Tones {
    fn from(s: &Scheme) -> Self {
        Self {
            accent: s.primary.into(),
            on_accent: s.on_primary.into(),
            surface: s.surface.into(),
            on_surface: s.on_surface.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The dominant colour picked from the wallpaper.
    pub source: Color,
    pub light: Tones,
    pub dark: Tones,
}

impl Palette {
    pub fn from_image(image: &DynamicImage) -> Self {
        let pixels: Vec<Rgb> = image
            .resize(SAMPLE_SIZE, SAMPLE_SIZE, FilterType::Triangle)
            .into_rgb8()
            .pixels()
            .map(|p| Rgb::new(p[0], p[1], p[2]))
            .collect();
        Self::from_source(extract_color(&pixels))
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self::from_image(&image::open(path)?))
    }

    fn from_source(source: Rgb) -> Self {
        let theme = ThemeBuilder::with_source(source).build();
        Self {
            source: source.into(),
            light: (&theme.schemes.light).into(),
            dark: (&theme.schemes.dark).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgb as Pixel, RgbImage};

    use super::*;

    fn solid(r: u8, g: u8, b: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(256, 144, Pixel([r, g, b])))
    }

    #[test]
    fn color_formats_as_lowercase_hex() {
        assert_eq!(
            Color {
                r: 200,
                g: 111,
                b: 9
            }
            .to_string(),
            "#c86f09"
        );
    }

    #[test]
    fn source_follows_the_dominant_colour() {
        let pink = Palette::from_image(&solid(200, 111, 201)).source;
        assert!(
            pink.r > pink.g && pink.b > pink.g,
            "expected a pink source, got {pink}"
        );

        let green = Palette::from_image(&solid(40, 180, 60)).source;
        assert!(
            green.g > green.r && green.g > green.b,
            "expected a green source, got {green}"
        );
    }

    #[test]
    fn dark_mode_surface_is_darker_than_light_mode_surface() {
        let p = Palette::from_image(&solid(200, 111, 201));
        let luma = |c: Color| u32::from(c.r) + u32::from(c.g) + u32::from(c.b);
        assert!(luma(p.dark.surface) < luma(p.light.surface));
        assert!(luma(p.dark.on_surface) > luma(p.dark.surface));
    }
}
