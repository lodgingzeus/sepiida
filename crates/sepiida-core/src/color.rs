//! sepiida's colour type: 8-bit sRGB, written and parsed as `#rrggbb`.

use std::{fmt, str::FromStr};

use material_colors::color::Rgb;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

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

    /// Black or white, whichever reads better as text on this colour.
    pub fn readable_text(self) -> Self {
        // L* 50 is about where black and white text have equal contrast.
        if lightness(self) > 50.0 {
            Self::new(0, 0, 0)
        } else {
            Self::new(255, 255, 255)
        }
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

/// CIE L* of an sRGB colour, 0 to 100.
pub(crate) fn lightness(c: Color) -> f32 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn readable_text_contrasts_with_the_background() {
        let black = Color::new(0, 0, 0);
        let white = Color::new(255, 255, 255);
        assert_eq!(white.readable_text(), black);
        assert_eq!(Color::new(240, 240, 240).readable_text(), black);
        assert_eq!(Color::new(242, 212, 173).readable_text(), black);
        assert_eq!(Color::new(255, 230, 0).readable_text(), black);
        assert_eq!(black.readable_text(), white);
        assert_eq!(Color::new(32, 32, 32).readable_text(), white);
        assert_eq!(Color::new(0, 0, 200).readable_text(), white);
    }
}
