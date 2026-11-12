//! The Windows accent colour, in the exact encodings the registry expects.
//!
//! Windows stores the same colour several times with different byte orders.
//! Keeping the conversions here, away from the registry code, lets them be
//! tested byte for byte on any platform.

use std::str::FromStr;

use material_colors::{color::Rgb, hct::Hct};

use crate::{Color, Palette};

/// Tones (0 = black, 100 = white) of the seven accent shades, lightest first.
/// The middle entry is the accent colour itself.
const SHADE_TONES: [f64; 7] = [86.0, 74.0, 62.0, 50.0, 40.0, 30.0, 20.0];
const ACCENT_INDEX: usize = 3;
/// Windows paints Start and the taskbar with the shade one step darker than
/// the accent colour.
const START_INDEX: usize = 4;
/// Alpha that Windows itself writes to `ColorizationColor`.
const COLORIZATION_ALPHA: u32 = 0xc4;

pub const ACCENT_PALETTE_LEN: usize = 32;

/// `0xAABBGGRR` with full alpha, the layout of `AccentColorMenu`,
/// `StartColorMenu` and DWM's `AccentColor`.
pub const fn to_abgr(c: Color) -> u32 {
    0xff00_0000 | (c.b as u32) << 16 | (c.g as u32) << 8 | c.r as u32
}

/// `0xAARRGGBB`, the layout of DWM's `ColorizationColor`.
pub const fn to_argb(c: Color, alpha: u8) -> u32 {
    (alpha as u32) << 24 | (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
}

/// Reads a `COLORREF` (`0x00BBGGRR`), as returned for a solid-colour desktop
/// background. The top byte is ignored.
pub const fn from_colorref(value: u32) -> Color {
    Color::new(value as u8, (value >> 8) as u8, (value >> 16) as u8)
}

/// Light or dark mode for apps and the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown mode `{0}`; expected light or dark")]
pub struct UnknownThemeMode(String);

impl FromStr for ThemeMode {
    type Err = UnknownThemeMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => Err(UnknownThemeMode(other.to_owned())),
        }
    }
}

/// Everything written to the registry for one accent colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentValues {
    /// `AccentPalette`: eight colours of four bytes each (red, green, blue,
    /// alpha). Seven shades from light to dark, then one contrasting colour.
    pub palette: [u8; ACCENT_PALETTE_LEN],
    pub accent_color_menu: u32,
    pub start_color_menu: u32,
    pub dwm_accent_color: u32,
    pub colorization_color: u32,
}

impl AccentValues {
    pub fn from_palette(palette: &Palette) -> Self {
        Self::from_color(palette.light.accent)
    }

    /// Builds the shades from the hue and colourfulness of `color`. Its
    /// brightness is ignored: the tones are fixed so that text drawn on the
    /// accent colour stays readable whatever the wallpaper looks like.
    pub fn from_color(color: Color) -> Self {
        let base = Hct::new(color.into());
        let (hue, chroma) = (base.get_hue(), base.get_chroma());
        let shade = |hue: f64, tone: f64| Color::from(Rgb::from(Hct::from(hue, chroma, tone)));

        let shades = SHADE_TONES.map(|tone| shade(hue, tone));
        let contrasting = shade((hue + 180.0) % 360.0, SHADE_TONES[ACCENT_INDEX]);

        let mut bytes = [0u8; ACCENT_PALETTE_LEN];
        let (slots, _) = bytes.as_chunks_mut::<4>();
        for (slot, c) in slots.iter_mut().zip(shades.iter().chain([&contrasting])) {
            *slot = [c.r, c.g, c.b, 0xff];
        }

        let accent = shades[ACCENT_INDEX];
        Self {
            palette: bytes,
            accent_color_menu: to_abgr(accent),
            start_color_menu: to_abgr(shades[START_INDEX]),
            dwm_accent_color: to_abgr(accent),
            colorization_color: COLORIZATION_ALPHA << 24 | (to_argb(accent, 0) & 0x00ff_ffff),
        }
    }

    pub fn accent(&self) -> Color {
        self.shade(ACCENT_INDEX)
    }

    fn shade(&self, index: usize) -> Color {
        let i = index * 4;
        Color::new(self.palette[i], self.palette[i + 1], self.palette[i + 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: Color = Color::new(0x12, 0x34, 0x56);

    fn luma(c: Color) -> u32 {
        u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114
    }

    #[test]
    fn mode_parses_light_and_dark_only() {
        assert_eq!("light".parse(), Ok(ThemeMode::Light));
        assert_eq!("dark".parse(), Ok(ThemeMode::Dark));
        assert!("auto".parse::<ThemeMode>().is_err());
    }

    #[test]
    fn byte_orders_match_the_registry_layouts() {
        assert_eq!(to_abgr(ORANGE), 0xff56_3412);
        assert_eq!(to_argb(ORANGE, 0xc4), 0xc412_3456);
        assert_eq!(to_abgr(ORANGE).to_le_bytes(), [0x12, 0x34, 0x56, 0xff]);
    }

    #[test]
    fn colorref_ignores_the_top_byte() {
        assert_eq!(from_colorref(0x0056_3412), ORANGE);
        assert_eq!(from_colorref(0xff56_3412), ORANGE);
        assert_eq!(from_colorref(0), Color::new(0, 0, 0));
        assert_eq!(from_colorref(to_abgr(ORANGE)), ORANGE);
    }

    #[test]
    fn palette_is_eight_opaque_rgba_colours_from_light_to_dark() {
        let v = AccentValues::from_color(Color::new(200, 111, 201));
        assert!(v.palette.as_chunks::<4>().0.iter().all(|c| c[3] == 0xff));
        for i in 0..6 {
            assert!(
                luma(v.shade(i)) > luma(v.shade(i + 1)),
                "shade {i} is not lighter than shade {}",
                i + 1
            );
        }
    }

    #[test]
    fn dword_values_agree_with_the_palette_entries() {
        let v = AccentValues::from_color(Color::new(40, 180, 60));
        let accent = v.accent();
        assert_eq!(&v.palette[12..16], &[accent.r, accent.g, accent.b, 0xff]);
        assert_eq!(v.accent_color_menu, to_abgr(accent));
        assert_eq!(v.dwm_accent_color, to_abgr(accent));
        assert_eq!(v.start_color_menu, to_abgr(v.shade(START_INDEX)));
        assert_eq!(v.colorization_color, to_argb(accent, 0xc4));
    }

    #[test]
    fn accent_keeps_the_hue_of_the_input() {
        let green = AccentValues::from_color(Color::new(40, 180, 60)).accent();
        assert!(green.g > green.r && green.g > green.b, "got {green}");
        let pink = AccentValues::from_color(Color::new(200, 111, 201)).accent();
        assert!(pink.r > pink.g && pink.b > pink.g, "got {pink}");
    }

    #[test]
    fn grey_black_and_white_inputs_still_produce_a_valid_ramp() {
        for c in [
            Color::new(0, 0, 0),
            Color::new(255, 255, 255),
            Color::new(128, 128, 128),
        ] {
            let v = AccentValues::from_color(c);
            assert!(luma(v.shade(0)) > luma(v.shade(6)), "input {c}");
        }
    }

    #[test]
    fn from_palette_uses_the_palette_accent() {
        let p = Palette::from_color(Color::new(40, 180, 60));
        assert_eq!(
            AccentValues::from_palette(&p),
            AccentValues::from_color(p.light.accent)
        );
    }
}
