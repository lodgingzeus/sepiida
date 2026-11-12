//! Platform-independent colour engine: wallpaper image in, palette out.

pub mod accent;
pub mod cache;
pub mod config;
pub mod engine_config;
pub mod engine_tracker;
mod fsio;
pub mod schedule;
pub mod snapshot;
pub mod source;
pub mod wallpaper_engine;

use std::{fmt, path::Path, str::FromStr};

use image::{DynamicImage, ImageReader, imageops::FilterType};
use material_colors::{color::Rgb, image::extract_color, scheme::Scheme, theme::ThemeBuilder};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Quantization time grows with pixel count and the result barely changes
/// above this size, so every image is shrunk to fit this box first.
const SAMPLE_SIZE: u32 = 128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to open image")]
    Io(#[from] std::io::Error),
    #[error("failed to decode image")]
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

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid colour `{0}`; expected #rrggbb")]
pub struct InvalidColor(String);

/// Parses `#rrggbb`, the format `Display` writes. Hex digits may be upper or
/// lower case.
impl FromStr for Color {
    type Err = InvalidColor;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidColor(s.to_owned());
        let hex = s.strip_prefix('#').ok_or_else(invalid)?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        // Six ASCII hex digits, so every two-byte slice is a valid number.
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| invalid());
        Ok(Self::new(channel(0)?, channel(2)?, channel(4)?))
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

impl From<Color> for Rgb {
    fn from(c: Color) -> Self {
        Self::new(c.r, c.g, c.b)
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
    /// How light the wallpaper looks overall: mean CIE L*, 0 (black) to
    /// 100 (white). For a solid colour, that colour's L*.
    pub lightness: u8,
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
        let lightness = mean_lightness(pixels.iter().map(|&p| Color::from(p)));
        Self::from_parts(extract_color(&pixels).into(), lightness)
    }

    /// The format is detected from the file contents, not the extension:
    /// the copy of the wallpaper that Windows keeps has no extension.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let reader = ImageReader::open(path)?.with_guessed_format()?;
        Ok(Self::from_image(&reader.decode()?))
    }

    /// For a solid-colour background, where there is no image to analyse.
    pub fn from_color(color: Color) -> Self {
        Self::from_parts(color, lightness(color).round() as u8)
    }

    pub fn from_wallpaper(wallpaper: &source::Wallpaper) -> Result<Self, Error> {
        match wallpaper {
            source::Wallpaper::Image(path) => Self::from_path(path),
            source::Wallpaper::Color(color) => Ok(Self::from_color(*color)),
        }
    }

    /// Rebuilds a palette from what the cache stores.
    pub fn from_parts(source: Color, lightness: u8) -> Self {
        let theme = ThemeBuilder::with_source(source.into()).build();
        Self {
            source,
            lightness: lightness.min(100),
            light: (&theme.schemes.light).into(),
            dark: (&theme.schemes.dark).into(),
        }
    }
}

/// CIE L* of an sRGB colour, 0 to 100.
fn lightness(c: Color) -> f32 {
    let linear = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b);
    if y > 216.0 / 24_389.0 {
        116.0 * y.cbrt() - 16.0
    } else {
        y * 24_389.0 / 27.0
    }
}

fn mean_lightness(pixels: impl Iterator<Item = Color>) -> u8 {
    let (sum, count) = pixels.fold((0.0f32, 0u32), |(sum, n), c| (sum + lightness(c), n + 1));
    if count == 0 {
        return 0;
    }
    // L* is within 0..=100, so the mean is too and the cast cannot wrap.
    (sum / count as f32).round().clamp(0.0, 100.0) as u8
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
    fn color_parses_what_it_formats() {
        let color = Color::new(200, 111, 9);
        assert_eq!(color.to_string().parse(), Ok(color));
        assert_eq!("#C86F09".parse(), Ok(color));
        for bad in [
            "",
            "#",
            "c86f09",
            "#c86f0",
            "#c86f091",
            "#c86f0g",
            "#+1+2+3",
            "#c86f\u{e9}",
        ] {
            assert!(bad.parse::<Color>().is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn palette_from_the_source_colour_matches_the_image_palette() {
        // The palette cache stores only the source colour and rebuilds the
        // rest from it, which is only correct if this holds.
        let from_image = Palette::from_image(&solid(200, 111, 201));
        assert_eq!(
            Palette::from_parts(from_image.source, from_image.lightness),
            from_image
        );
    }

    #[test]
    fn lightness_runs_from_black_to_white() {
        assert_eq!(Palette::from_image(&solid(0, 0, 0)).lightness, 0);
        assert_eq!(Palette::from_image(&solid(255, 255, 255)).lightness, 100);
        let grey = Palette::from_image(&solid(119, 119, 119)).lightness;
        assert!((49..=51).contains(&grey), "mid grey gave {grey}");
        assert_eq!(
            Palette::from_color(Color::new(255, 255, 255)).lightness,
            100
        );
        assert_eq!(mean_lightness(std::iter::empty()), 0);
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
    fn from_path_detects_the_format_without_an_extension() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("TranscodedWallpaper");
        solid(40, 180, 60)
            .save_with_format(&path, image::ImageFormat::Png)
            .expect("write image");
        let from_file = Palette::from_path(&path).expect("palette");
        assert_eq!(from_file, Palette::from_image(&solid(40, 180, 60)));
    }

    #[test]
    fn from_path_reports_missing_and_corrupt_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            Palette::from_path(dir.path().join("missing.png")),
            Err(Error::Io(_))
        ));
        let corrupt = dir.path().join("corrupt.png");
        std::fs::write(&corrupt, b"not an image").expect("write file");
        assert!(matches!(Palette::from_path(&corrupt), Err(Error::Image(_))));
    }

    #[test]
    fn from_color_keeps_the_colour_as_source() {
        let color = Color::new(40, 180, 60);
        assert_eq!(Palette::from_color(color).source, color);
        assert_eq!(
            Palette::from_wallpaper(&source::Wallpaper::Color(color)).expect("palette"),
            Palette::from_color(color)
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
