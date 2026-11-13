//! The Windows accent colour, in the exact encodings the registry expects.
//!
//! Windows stores the same colour several times with different byte orders.
//! Keeping the conversions here, away from the registry code, lets them be
//! tested byte for byte on any platform.

use std::{fmt, str::FromStr};

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

/// Source chroma (HCT) at or below which the wallpaper counts as grey.
const GREY_CHROMA: f64 = 4.0;
/// Source chroma from which the wallpaper gets the full Material accent.
const COLOURFUL_CHROMA: f64 = 16.0;

impl AccentValues {
    /// Uses the hue of the palette's accent, with its colourfulness scaled
    /// by how colourful the wallpaper is: the Material accent always has
    /// strong chroma, so a grey or black wallpaper would otherwise get a
    /// loud accent in an arbitrary hue. Grey sources get a neutral accent,
    /// clearly coloured ones the full Material accent, pale tints in
    /// between.
    pub fn from_palette(palette: &Palette) -> Self {
        let accent = Hct::new(palette.light.accent.into());
        let source = Hct::new(palette.source.into()).get_chroma();
        let share = ((source - GREY_CHROMA) / (COLOURFUL_CHROMA - GREY_CHROMA)).clamp(0.0, 1.0);
        let chroma = source.max(accent.get_chroma() * share);
        Self::from_hue_chroma(accent.get_hue(), chroma.min(accent.get_chroma()))
    }

    /// Builds the shades from the hue and colourfulness of `color`. Its
    /// brightness is ignored: the tones are fixed so that text drawn on the
    /// accent colour stays readable whatever the wallpaper looks like.
    pub fn from_color(color: Color) -> Self {
        let base = Hct::new(color.into());
        Self::from_hue_chroma(base.get_hue(), base.get_chroma())
    }

    fn from_hue_chroma(hue: f64, chroma: f64) -> Self {
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

const ACCENT_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
const DWM_KEY: &str = r"Software\Microsoft\Windows\DWM";
const DESKTOP_KEY: &str = r"Control Panel\Desktop";
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryData {
    Dword(u32),
    Binary(Vec<u8>),
}

impl fmt::Display for RegistryData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dword(value) => write!(f, "0x{value:08x}"),
            Self::Binary(bytes) => bytes.iter().try_for_each(|b| write!(f, "{b:02x}")),
        }
    }
}

/// One value to set under `HKEY_CURRENT_USER`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryWrite {
    pub key: &'static str,
    pub name: &'static str,
    pub data: RegistryData,
}

impl RegistryWrite {
    const fn dword(key: &'static str, name: &'static str, value: u32) -> Self {
        Self {
            key,
            name,
            data: RegistryData::Dword(value),
        }
    }
}

impl AccentValues {
    pub fn registry_writes(&self) -> Vec<RegistryWrite> {
        vec![
            RegistryWrite {
                key: ACCENT_KEY,
                name: "AccentPalette",
                data: RegistryData::Binary(self.palette.to_vec()),
            },
            RegistryWrite::dword(ACCENT_KEY, "AccentColorMenu", self.accent_color_menu),
            RegistryWrite::dword(ACCENT_KEY, "StartColorMenu", self.start_color_menu),
            RegistryWrite::dword(DWM_KEY, "AccentColor", self.dwm_accent_color),
            RegistryWrite::dword(DWM_KEY, "ColorizationColor", self.colorization_color),
            RegistryWrite::dword(DWM_KEY, "ColorizationAfterglow", self.colorization_color),
            // Stops Windows from replacing our accent with its own pick from
            // the wallpaper.
            RegistryWrite::dword(DESKTOP_KEY, "AutoColorization", 0),
        ]
    }
}

impl ThemeMode {
    pub fn registry_writes(self) -> Vec<RegistryWrite> {
        let light = u32::from(self == Self::Light);
        vec![
            RegistryWrite::dword(PERSONALIZE_KEY, "AppsUseLightTheme", light),
            RegistryWrite::dword(PERSONALIZE_KEY, "SystemUsesLightTheme", light),
        ]
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

    fn find<'a>(writes: &'a [RegistryWrite], name: &str) -> &'a RegistryData {
        match writes.iter().find(|w| w.name == name) {
            Some(write) => &write.data,
            None => panic!("no write for {name}"),
        }
    }

    #[test]
    fn accent_writes_carry_the_values_under_their_keys() {
        let v = AccentValues::from_color(Color::new(40, 180, 60));
        let writes = v.registry_writes();
        assert_eq!(
            find(&writes, "AccentPalette"),
            &RegistryData::Binary(v.palette.to_vec())
        );
        assert_eq!(
            find(&writes, "AccentColorMenu"),
            &RegistryData::Dword(v.accent_color_menu)
        );
        assert_eq!(
            find(&writes, "ColorizationColor"),
            &RegistryData::Dword(v.colorization_color)
        );
        assert_eq!(find(&writes, "AutoColorization"), &RegistryData::Dword(0));
        assert!(
            writes
                .iter()
                .all(|w| !w.key.starts_with('\\') && !w.key.ends_with('\\'))
        );
        let mut names: Vec<_> = writes.iter().map(|w| (w.key, w.name)).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), writes.len(), "duplicate registry value");
    }

    #[test]
    fn mode_writes_set_both_light_theme_flags() {
        for (mode, flag) in [(ThemeMode::Light, 1), (ThemeMode::Dark, 0)] {
            let writes = mode.registry_writes();
            assert_eq!(
                find(&writes, "AppsUseLightTheme"),
                &RegistryData::Dword(flag)
            );
            assert_eq!(
                find(&writes, "SystemUsesLightTheme"),
                &RegistryData::Dword(flag)
            );
        }
    }

    #[test]
    fn registry_data_prints_as_hex() {
        assert_eq!(RegistryData::Dword(0xff56_3412).to_string(), "0xff563412");
        assert_eq!(
            RegistryData::Binary(vec![0, 0x1f, 0xff]).to_string(),
            "001fff"
        );
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
    fn from_palette_uses_the_palette_accent_for_colourful_wallpapers() {
        let p = Palette::from_color(Color::new(40, 180, 60));
        assert_eq!(
            AccentValues::from_palette(&p),
            AccentValues::from_color(p.light.accent)
        );
    }

    /// Below this an accent reads as grey.
    const NEUTRAL_CHROMA: f64 = 4.0;

    fn chroma(c: Color) -> f64 {
        Hct::new(c.into()).get_chroma()
    }

    #[test]
    fn grey_and_black_wallpapers_get_a_neutral_accent() {
        for source in [
            Color::new(0, 0, 0),
            Color::new(128, 128, 128),
            Color::new(240, 240, 240),
            Color::new(30, 32, 34),
        ] {
            let p = Palette::from_color(source);
            assert!(chroma(p.light.accent) > 20.0, "Material adds colour");
            let accent = AccentValues::from_palette(&p).accent();
            // Pure greys still measure a little chroma in CAM16.
            assert!(chroma(accent) < NEUTRAL_CHROMA, "{source} gave {accent}");
        }
    }

    #[test]
    fn muted_wallpapers_get_a_muted_accent_in_their_own_hue() {
        // A desaturated teal, like an overcast sea photo.
        let source = Color::new(96, 128, 128);
        let p = Palette::from_color(source);
        let accent = AccentValues::from_palette(&p).accent();
        assert!(chroma(accent) < chroma(p.light.accent), "got {accent}");
        assert!(chroma(accent) > NEUTRAL_CHROMA, "got {accent}");
        assert!(accent.g > accent.r && accent.b > accent.r, "got {accent}");
    }
}
