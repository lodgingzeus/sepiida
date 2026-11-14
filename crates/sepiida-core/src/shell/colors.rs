//! The colour of each role for the shell, from the palette and the
//! system's light/dark mode.
//!
//! The mode is the one the taskbar follows ("Windows mode",
//! `SystemUsesLightTheme`), not the app mode: the taskbar draws its text and
//! icons for that mode, so a light surface under white text would be
//! unreadable.

use material_colors::{color::Rgb, hct::Hct};

use super::rules::Role;
use crate::{
    Color, Palette,
    accent::{AccentValues, hue_and_chroma},
};

/// Tone (HCT lightness) of the surface in dark mode: close to the stock
/// dark taskbar (about L* 12), a little lighter so the hue shows.
pub const DARK_SURFACE_TONE: f64 = 16.0;
/// Tone of the surface in light mode, close to the stock light taskbar.
pub const LIGHT_SURFACE_TONE: f64 = 92.0;
/// Most colourfulness a surface gets. Large areas in the full accent chroma
/// look loud; pale surfaces need less to show the same hue.
pub const DARK_SURFACE_CHROMA: f64 = 24.0;
pub const LIGHT_SURFACE_CHROMA: f64 = 16.0;
/// Tone of cards on a surface in dark mode: a step above the surface, as
/// Windows' dark cards sit above their panel.
pub const DARK_RAISED_TONE: f64 = 24.0;
/// Tone of cards in light mode: near white, above the light surface.
pub const LIGHT_RAISED_TONE: f64 = 98.0;
/// Cards hold text, so they get less colour than the surface.
pub const DARK_RAISED_CHROMA: f64 = 16.0;
pub const LIGHT_RAISED_CHROMA: f64 = 8.0;

/// The colour of every role, in `Role::ALL` order.
pub fn role_colors(palette: &Palette, system_dark: bool) -> Vec<(Role, Color)> {
    Role::ALL
        .into_iter()
        .map(|role| (role, role_color(palette, system_dark, role)))
        .collect()
}

pub fn role_color(palette: &Palette, system_dark: bool, role: Role) -> Color {
    match role {
        Role::Surface if system_dark => toned(palette, DARK_SURFACE_TONE, DARK_SURFACE_CHROMA),
        Role::Surface => toned(palette, LIGHT_SURFACE_TONE, LIGHT_SURFACE_CHROMA),
        Role::SurfaceRaised if system_dark => toned(palette, DARK_RAISED_TONE, DARK_RAISED_CHROMA),
        Role::SurfaceRaised => toned(palette, LIGHT_RAISED_TONE, LIGHT_RAISED_CHROMA),
        Role::Accent => AccentValues::from_palette(palette).accent(),
    }
}

/// The palette's hue at `tone`, with at most `max_chroma` colourfulness.
fn toned(palette: &Palette, tone: f64, max_chroma: f64) -> Color {
    let (hue, chroma) = hue_and_chroma(palette);
    Color::from(Rgb::from(Hct::from(hue, chroma.min(max_chroma), tone)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::lightness;

    fn chroma(c: Color) -> f64 {
        Hct::new(c.into()).get_chroma()
    }

    fn hue(c: Color) -> f64 {
        Hct::new(c.into()).get_hue()
    }

    #[test]
    fn the_surface_is_dark_for_dark_mode_and_light_for_light_mode() {
        let palette = Palette::from_color(Color::new(0x33, 0x66, 0x99));
        let dark = role_color(&palette, true, Role::Surface);
        let light = role_color(&palette, false, Role::Surface);
        assert!((lightness(dark) - DARK_SURFACE_TONE as f32).abs() < 2.0);
        assert!((lightness(light) - LIGHT_SURFACE_TONE as f32).abs() < 2.0);
    }

    #[test]
    fn a_colourful_wallpaper_tints_the_surface_with_its_hue() {
        let blue = Color::new(0x20, 0x50, 0xd0);
        let palette = Palette::from_color(blue);
        let accent = role_color(&palette, true, Role::Accent);
        for system_dark in [true, false] {
            let surface = role_color(&palette, system_dark, Role::Surface);
            assert!(chroma(surface) > 8.0, "{surface} is too grey");
            assert!((hue(surface) - hue(accent)).abs() < 15.0);
        }
        assert!(chroma(role_color(&palette, true, Role::Surface)) <= DARK_SURFACE_CHROMA + 1.0);
        assert!(chroma(role_color(&palette, false, Role::Surface)) <= LIGHT_SURFACE_CHROMA + 1.0);
    }

    #[test]
    fn grey_and_black_wallpapers_give_a_neutral_surface() {
        for grey in [Color::new(0, 0, 0), Color::new(0x80, 0x80, 0x80)] {
            let palette = Palette::from_color(grey);
            for (system_dark, role) in [true, false]
                .into_iter()
                .flat_map(|dark| [(dark, Role::Surface), (dark, Role::SurfaceRaised)])
            {
                let surface = role_color(&palette, system_dark, role);
                // HCT reports a small chroma even for exact greys, so check
                // the channels instead.
                let spread = [surface.r, surface.g, surface.b]
                    .iter()
                    .fold((u8::MAX, 0u8), |(lo, hi), &c| (lo.min(c), hi.max(c)));
                assert!(spread.1 - spread.0 <= 2, "{surface} from {grey} is tinted");
            }
        }
    }

    #[test]
    fn cards_are_a_step_lighter_than_the_surface_with_its_hue() {
        let palette = Palette::from_color(Color::new(0x20, 0x50, 0xd0));
        for system_dark in [true, false] {
            let surface = role_color(&palette, system_dark, Role::Surface);
            let raised = role_color(&palette, system_dark, Role::SurfaceRaised);
            assert_ne!(surface, raised);
            assert!(
                lightness(raised) > lightness(surface) + 4.0,
                "{raised} vs {surface}"
            );
            assert!((hue(raised) - hue(surface)).abs() < 15.0);
        }
        let dark = role_color(&palette, true, Role::SurfaceRaised);
        let light = role_color(&palette, false, Role::SurfaceRaised);
        // White text on dark cards, dark text on light ones.
        assert!(lightness(dark) < 35.0, "{dark}");
        assert!(lightness(light) > 90.0, "{light}");
        assert!(chroma(dark) <= DARK_RAISED_CHROMA + 1.0);
        assert!(chroma(light) <= LIGHT_RAISED_CHROMA + 1.0);
    }

    #[test]
    fn the_accent_role_is_the_windows_accent() {
        let palette = Palette::from_color(Color::new(0xc0, 0x40, 0x40));
        assert_eq!(
            role_color(&palette, true, Role::Accent),
            AccentValues::from_palette(&palette).accent()
        );
    }

    #[test]
    fn every_role_gets_a_colour() {
        let palette = Palette::from_color(Color::new(1, 2, 3));
        let roles: Vec<Role> = role_colors(&palette, true)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(roles, Role::ALL);
    }
}
