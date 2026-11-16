//! The palette engine: wallpaper image (or solid colour) in, Material You
//! palette out.
//!
//! Analysing an image gives up to four candidate colours, best first, and
//! its lightness. The palette is then built from one candidate (or any
//! other colour the user picks) in one of four styles.

use std::path::Path;

use image::{DynamicImage, ImageReader, imageops::FilterType};
use material_colors::{
    color::Rgb,
    dynamic_color::Variant,
    quantize::{Quantizer, QuantizerCelebi},
    scheme::Scheme,
    score::Score,
    theme::ThemeBuilder,
};
use serde::{Deserialize, Serialize};

use crate::{Color, color::lightness, source};

/// Quantization time grows with pixel count and the result barely changes
/// above this size, so every image is shrunk to fit this box first.
const SAMPLE_SIZE: u32 = 128;
/// Colours the quantizer reduces an image to before scoring.
const MAX_QUANTIZED_COLORS: usize = 128;
/// Colours offered to choose from, like Android's wallpaper colours.
pub const MAX_CANDIDATES: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to open image")]
    Io(#[from] std::io::Error),
    #[error("failed to decode image")]
    Image(#[from] image::ImageError),
}

/// How the palette treats its source colour (Material's scheme variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    /// Keeps the source colour as it is (Material "fidelity").
    Faithful,
    /// Calm, slightly muted colours (Material "tonal spot"). The default,
    /// and what sepiida used before styles existed.
    #[default]
    Soft,
    /// Stronger colours than the source (Material "vibrant").
    Vivid,
    /// Nearly grey, with a hint of the source's hue (Material "neutral").
    Neutral,
}

impl Style {
    pub const ALL: [Self; 4] = [Self::Faithful, Self::Soft, Self::Vivid, Self::Neutral];

    const fn variant(self) -> Variant {
        match self {
            Self::Faithful => Variant::Fidelity,
            Self::Soft => Variant::TonalSpot,
            Self::Vivid => Variant::Vibrant,
            Self::Neutral => Variant::Neutral,
        }
    }
}

/// The colours found in a wallpaper, best first. Never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidates {
    colors: [Color; MAX_CANDIDATES],
    len: usize,
}

impl Candidates {
    pub const fn one(color: Color) -> Self {
        Self {
            colors: [color; MAX_CANDIDATES],
            len: 1,
        }
    }

    /// Takes the first `MAX_CANDIDATES` colours, skipping repeats. `None`
    /// for an empty list.
    pub fn from_colors(colors: impl IntoIterator<Item = Color>) -> Option<Self> {
        let mut colors = colors.into_iter();
        let mut candidates = Self::one(colors.next()?);
        for color in colors {
            if candidates.len == MAX_CANDIDATES {
                break;
            }
            if !candidates.as_slice().contains(&color) {
                candidates.colors[candidates.len] = color;
                candidates.len += 1;
            }
        }
        Some(candidates)
    }

    pub const fn best(&self) -> Color {
        self.colors[0]
    }

    pub fn as_slice(&self) -> &[Color] {
        &self.colors[..self.len]
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
    /// The colour the palette is built from: by default the best colour
    /// found in the wallpaper.
    pub source: Color,
    pub style: Style,
    /// The colours found in the wallpaper, to choose `source` from.
    pub candidates: Candidates,
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
        Self::from_parts(candidates(&pixels), lightness)
    }

    /// The format is detected from the file contents, not the extension:
    /// the copy of the wallpaper that Windows keeps has no extension.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let reader = ImageReader::open(path)?.with_guessed_format()?;
        Ok(Self::from_image(&reader.decode()?))
    }

    /// For a solid-colour background, where there is no image to analyse.
    pub fn from_color(color: Color) -> Self {
        Self::from_parts(Candidates::one(color), lightness(color).round() as u8)
    }

    pub fn from_wallpaper(wallpaper: &source::Wallpaper) -> Result<Self, Error> {
        match wallpaper {
            source::Wallpaper::Image(path) => Self::from_path(path),
            source::Wallpaper::Color(color) => Ok(Self::from_color(*color)),
        }
    }

    /// Rebuilds a palette from what the cache stores, in the default style
    /// from the best candidate.
    pub fn from_parts(candidates: Candidates, lightness: u8) -> Self {
        Self::build(candidates.best(), Style::default(), candidates, lightness)
    }

    /// The same wallpaper's palette built from `source` in `style`.
    #[must_use]
    pub fn with_choice(&self, source: Color, style: Style) -> Self {
        Self::build(source, style, self.candidates, self.lightness)
    }

    fn build(source: Color, style: Style, candidates: Candidates, lightness: u8) -> Self {
        let theme = ThemeBuilder::with_source(source.into())
            .variant(style.variant())
            .build();
        Self {
            source,
            style,
            candidates,
            lightness: lightness.min(100),
            light: (&theme.schemes.light).into(),
            dark: (&theme.schemes.dark).into(),
        }
    }
}

/// The colours to offer, best first. Material scoring prefers colourful,
/// widely used colours and skips near-greys; when nothing scores (a grey,
/// black or very dark wallpaper) the most common colour is used instead of
/// the library's fixed blue fallback, so the result still looks like the
/// wallpaper.
fn candidates(pixels: &[Rgb]) -> Candidates {
    let quantized = QuantizerCelebi::quantize(pixels, MAX_QUANTIZED_COLORS).color_to_count;
    let Some(dominant) = quantized
        .iter()
        .max_by_key(|&(_, &count)| count)
        .map(|(&color, _)| color)
    else {
        return Candidates::one(Color::new(0, 0, 0));
    };
    let scored = Score::score(&quantized, Some(MAX_CANDIDATES), Some(dominant), None);
    Candidates::from_colors(scored.into_iter().map(Color::from))
        .unwrap_or_else(|| Candidates::one(dominant.into()))
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
mod tests;
