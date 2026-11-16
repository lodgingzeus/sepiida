//! Colours of sepiida's own settings window, from the palette in effect,
//! so the window wears the colours it is about to give the shell.
//!
//! Every role has a fixed tone, so text contrast holds whatever the
//! wallpaper; only hue and colourfulness follow the palette (through
//! `hue_and_chroma`, so a grey wallpaper gives a grey window).

use material_colors::{color::Rgb, hct::Hct};

use crate::{
    Color, Palette,
    accent::{AccentValues, hue_and_chroma},
    shell::{colors::role_color, rules::Role},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowColors {
    pub background: Color,
    /// Grouped settings and the preview frame.
    pub card: Color,
    /// Selected chips and the tracks of segmented buttons.
    pub chip: Color,
    pub outline: Color,
    pub text: Color,
    pub muted_text: Color,
    /// Selected buttons and switches.
    pub accent: Color,
    pub on_accent: Color,
}

/// (tone, most chroma) per role, light then dark.
struct Spec {
    background: (f64, f64),
    card: (f64, f64),
    chip: (f64, f64),
    outline: (f64, f64),
    text: (f64, f64),
    muted_text: (f64, f64),
    accent: (f64, f64),
    on_accent: (f64, f64),
}

const LIGHT: Spec = Spec {
    background: (98.0, 6.0),
    card: (95.0, 10.0),
    chip: (89.0, 18.0),
    outline: (80.0, 10.0),
    text: (10.0, 4.0),
    muted_text: (32.0, 8.0),
    accent: (40.0, 60.0),
    on_accent: (100.0, 0.0),
};

const DARK: Spec = Spec {
    background: (8.0, 6.0),
    card: (13.0, 10.0),
    chip: (24.0, 16.0),
    outline: (32.0, 10.0),
    text: (92.0, 4.0),
    muted_text: (76.0, 8.0),
    accent: (80.0, 48.0),
    on_accent: (20.0, 16.0),
};

impl WindowColors {
    pub fn new(palette: &Palette, dark: bool) -> Self {
        let (hue, chroma) = hue_and_chroma(palette);
        let tone = |(tone, max_chroma): (f64, f64)| {
            Color::from(Rgb::from(Hct::from(hue, chroma.min(max_chroma), tone)))
        };
        let spec = if dark { &DARK } else { &LIGHT };
        Self {
            background: tone(spec.background),
            card: tone(spec.card),
            chip: tone(spec.chip),
            outline: tone(spec.outline),
            text: tone(spec.text),
            muted_text: tone(spec.muted_text),
            accent: tone(spec.accent),
            on_accent: tone(spec.on_accent),
        }
    }
}

/// What a palette choice chip shows: the Windows accent on top, the light
/// and dark shell surfaces below, as that colour would give them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipColors {
    pub accent: Color,
    pub light: Color,
    pub dark: Color,
}

/// One chip per colour found in the wallpaper, in the palette's style.
pub fn chip_colors(wallpaper: &Palette) -> Vec<ChipColors> {
    wallpaper
        .candidates
        .as_slice()
        .iter()
        .map(|&color| {
            let p = wallpaper.with_choice(color, wallpaper.style);
            ChipColors {
                accent: AccentValues::from_palette(&p).accent(),
                light: role_color(&p, false, Role::SurfaceRaised),
                dark: role_color(&p, true, Role::Surface),
            }
        })
        .collect()
}

/// The colours the preview draws the taskbar and Start in: the ones the
/// shell gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellPreview {
    pub surface: Color,
    pub card: Color,
    pub accent: Color,
}

impl ShellPreview {
    pub fn new(palette: &Palette, dark: bool) -> Self {
        Self {
            surface: role_color(palette, dark, Role::Surface),
            card: role_color(palette, dark, Role::SurfaceRaised),
            accent: role_color(palette, dark, Role::Accent),
        }
    }
}

/// Colours along the picker's hue track, left (hue 0) to right (360).
pub fn hue_stops() -> [Color; 8] {
    std::array::from_fn(|i| picker_color(i as f64 * 360.0 / 7.0, 60.0))
}

/// `color` at a fixed tone, for the custom colour picker: the accent
/// ignores brightness, so the picker offers hue and colourfulness only.
pub fn picker_color(hue: f64, chroma: f64) -> Color {
    Color::from(Rgb::from(Hct::from(
        hue.rem_euclid(360.0),
        chroma.clamp(0.0, MAX_PICKER_CHROMA),
        PICKER_TONE,
    )))
}

/// Hue and colourfulness of `color`, to start the picker from it.
pub fn picker_position(color: Color) -> (f64, f64) {
    let hct = Hct::new(color.into());
    (hct.get_hue(), hct.get_chroma().min(MAX_PICKER_CHROMA))
}

/// Tone of the picker's colours: mid, like the Windows accent.
pub const PICKER_TONE: f64 = 50.0;
/// Most colourfulness the picker offers; about the most sRGB holds at
/// tone 50 for most hues.
pub const MAX_PICKER_CHROMA: f64 = 80.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG contrast ratio.
    fn contrast(a: Color, b: Color) -> f64 {
        let luminance = |c: Color| {
            let channel = |v: u8| {
                let v = f64::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn chroma(c: Color) -> f64 {
        Hct::new(c.into()).get_chroma()
    }

    const SOURCES: [Color; 5] = [
        Color::new(200, 60, 40),
        Color::new(40, 90, 200),
        Color::new(240, 220, 60),
        Color::new(128, 128, 128),
        Color::new(0, 0, 0),
    ];

    #[test]
    fn text_stays_readable_on_every_surface() {
        for source in SOURCES {
            let palette = Palette::from_color(source);
            for dark in [false, true] {
                let c = WindowColors::new(&palette, dark);
                for surface in [c.background, c.card, c.chip] {
                    assert!(contrast(c.text, surface) >= 7.0, "{source} dark={dark}");
                    assert!(
                        contrast(c.muted_text, surface) >= 4.5,
                        "{source} dark={dark}"
                    );
                }
                assert!(
                    contrast(c.on_accent, c.accent) >= 4.5,
                    "{source} dark={dark}"
                );
            }
        }
    }

    #[test]
    fn grey_wallpapers_give_a_grey_window() {
        let c = WindowColors::new(&Palette::from_color(Color::new(128, 128, 128)), false);
        for color in [c.background, c.card, c.chip, c.accent] {
            assert!(chroma(color) < 4.0, "got {color}");
        }
    }

    #[test]
    fn chips_follow_the_candidates_and_the_style() {
        let wallpaper = Palette::from_parts(
            crate::Candidates::from_colors([Color::new(200, 60, 40), Color::new(40, 90, 200)])
                .expect("not empty"),
            40,
        );
        let soft = chip_colors(&wallpaper);
        assert_eq!(soft.len(), 2);
        assert_ne!(soft[0].accent, soft[1].accent);
        let neutral = chip_colors(&wallpaper.with_choice(wallpaper.source, crate::Style::Neutral));
        assert!(chroma(neutral[0].accent) < chroma(soft[0].accent));
        assert_eq!(
            soft[0].accent,
            ShellPreview::new(&wallpaper, false).accent,
            "the chip in use shows the accent the shell gets"
        );
    }

    #[test]
    fn hue_stops_wrap_around() {
        let stops = hue_stops();
        assert_eq!(stops[0], stops[7]);
        assert_ne!(stops[0], stops[3]);
    }

    #[test]
    fn the_picker_round_trips_its_position() {
        let color = picker_color(250.0, 40.0);
        let (hue, chroma) = picker_position(color);
        assert!(
            (hue - 250.0).abs() < 2.0 && (chroma - 40.0).abs() < 2.0,
            "{hue} {chroma}"
        );
        assert_eq!(picker_color(-110.0, 40.0), color, "hue wraps around");
        assert!(picker_position(picker_color(10.0, 500.0)).1 <= MAX_PICKER_CHROMA + 1.0);
    }
}
