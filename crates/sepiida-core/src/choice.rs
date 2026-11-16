//! Which colour the palette is built from, and in which style: the
//! wallpaper's best colour, another of its colours picked for that one
//! wallpaper, or a custom colour. The agent, `sepiida apply` and the
//! settings preview all go through `ColorChoice::palette`, so what the
//! window shows is what the shell gets.

use serde::{Deserialize, Serialize};

use crate::{Color, Palette, Style, cache::CacheKey, source::Wallpaper};

/// A colour picked from one wallpaper's candidates. It applies only while
/// that wallpaper is shown; another wallpaper gets its own best colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WallpaperPick {
    /// The wallpaper file: path, modification time and size. The path
    /// alone is not enough: Windows' own copy of the wallpaper
    /// (`TranscodedWallpaper`) keeps one path for every picture.
    pub wallpaper: CacheKey,
    pub color: Color,
}

impl WallpaperPick {
    /// `None` for a solid-colour background, which has only one colour,
    /// or a file that cannot be inspected.
    pub fn new(wallpaper: &Wallpaper, color: Color) -> Option<Self> {
        Some(Self {
            wallpaper: wallpaper_key(wallpaper)?,
            color,
        })
    }

    fn applies_to(&self, wallpaper: &Wallpaper) -> bool {
        wallpaper_key(wallpaper).is_some_and(|key| key == self.wallpaper)
    }
}

fn wallpaper_key(wallpaper: &Wallpaper) -> Option<CacheKey> {
    match wallpaper {
        Wallpaper::Image(path) => CacheKey::for_file(path),
        Wallpaper::Color(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColorChoice {
    pub style: Style,
    /// Used instead of the wallpaper's colours when set.
    pub custom: Option<Color>,
    pub pick: Option<WallpaperPick>,
}

impl ColorChoice {
    /// The palette to apply. `wallpaper` is the wallpaper and its own
    /// palette, or `None` when it could not be read; then only a custom
    /// colour gives a palette. A custom colour keeps the wallpaper's
    /// lightness when it is known, so "follow the wallpaper" still follows
    /// the picture rather than the colour.
    pub fn palette(&self, wallpaper: Option<(&Wallpaper, &Palette)>) -> Option<Palette> {
        let (source, base) = match (self.custom, wallpaper) {
            (Some(custom), Some((_, palette))) => (custom, *palette),
            (Some(custom), None) => (custom, Palette::from_color(custom)),
            (None, Some((wallpaper, palette))) => {
                (self.wallpaper_source(wallpaper, palette), *palette)
            }
            (None, None) => return None,
        };
        Some(base.with_choice(source, self.style))
    }

    /// Whether the colour in use comes from the wallpaper's candidates
    /// (`Some(index)`) rather than a custom colour.
    pub fn candidate_index(&self, wallpaper: &Wallpaper, palette: &Palette) -> Option<usize> {
        if self.custom.is_some() {
            return None;
        }
        let source = self.wallpaper_source(wallpaper, palette);
        palette
            .candidates
            .as_slice()
            .iter()
            .position(|&c| c == source)
    }

    fn wallpaper_source(&self, wallpaper: &Wallpaper, palette: &Palette) -> Color {
        self.pick
            .as_ref()
            .filter(|pick| pick.applies_to(wallpaper))
            .map_or(palette.candidates.best(), |pick| pick.color)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::Candidates;

    const RED: Color = Color::new(200, 60, 40);
    const BLUE: Color = Color::new(40, 90, 200);
    const CUSTOM: Color = Color::new(20, 160, 150);

    /// The pick only looks at the file's path, time and size, so any
    /// bytes will do.
    fn image(dir: &Path, name: &str, bytes: &[u8]) -> Wallpaper {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write file");
        Wallpaper::Image(path)
    }

    fn two_colours() -> Palette {
        Palette::from_parts(Candidates::from_colors([RED, BLUE]).expect("not empty"), 30)
    }

    #[test]
    fn by_default_the_best_colour_in_the_default_style() {
        let dir = tempfile::tempdir().expect("temp dir");
        let palette = two_colours();
        let got =
            ColorChoice::default().palette(Some((&image(dir.path(), "a.jpg", b"a"), &palette)));
        assert_eq!(got, Some(palette));
    }

    #[test]
    fn a_pick_applies_only_to_its_own_wallpaper() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (a, b) = (
            image(dir.path(), "a.jpg", b"a"),
            image(dir.path(), "b.jpg", b"b"),
        );
        let palette = two_colours();
        let choice = ColorChoice {
            pick: WallpaperPick::new(&a, BLUE),
            ..ColorChoice::default()
        };
        let on = |wallpaper| {
            choice
                .palette(Some((wallpaper, &palette)))
                .expect("palette")
                .source
        };
        assert_eq!(on(&a), BLUE);
        assert_eq!(on(&b), RED);
        assert_eq!(choice.candidate_index(&a, &palette), Some(1));
        assert_eq!(choice.candidate_index(&b, &palette), Some(0));
    }

    #[test]
    fn a_new_picture_at_the_same_path_drops_the_pick() {
        // Like TranscodedWallpaper, which Windows rewrites in place.
        let dir = tempfile::tempdir().expect("temp dir");
        let first = image(dir.path(), "TranscodedWallpaper", b"first");
        let choice = ColorChoice {
            pick: WallpaperPick::new(&first, BLUE),
            ..ColorChoice::default()
        };
        let second = image(dir.path(), "TranscodedWallpaper", b"the second picture");
        let palette = two_colours();
        assert_eq!(
            choice
                .palette(Some((&second, &palette)))
                .expect("palette")
                .source,
            RED
        );
    }

    #[test]
    fn solid_backgrounds_and_missing_files_cannot_be_picked_for() {
        assert_eq!(WallpaperPick::new(&Wallpaper::Color(RED), RED), None);
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = Wallpaper::Image(dir.path().join("missing.jpg"));
        assert_eq!(WallpaperPick::new(&missing, RED), None);
    }

    #[test]
    fn a_custom_colour_wins_and_keeps_the_wallpaper_lightness() {
        let dir = tempfile::tempdir().expect("temp dir");
        let a = image(dir.path(), "a.jpg", b"a");
        let palette = two_colours();
        let choice = ColorChoice {
            custom: Some(CUSTOM),
            pick: WallpaperPick::new(&a, BLUE),
            style: Style::Vivid,
        };
        let got = choice.palette(Some((&a, &palette))).expect("palette");
        assert_eq!(
            (got.source, got.style, got.lightness),
            (CUSTOM, Style::Vivid, 30)
        );
        assert_eq!(choice.candidate_index(&a, &palette), None);
    }

    #[test]
    fn without_a_wallpaper_only_a_custom_colour_gives_a_palette() {
        assert_eq!(ColorChoice::default().palette(None), None);
        let choice = ColorChoice {
            custom: Some(CUSTOM),
            ..ColorChoice::default()
        };
        assert_eq!(choice.palette(None).expect("palette").source, CUSTOM);
    }

    #[test]
    fn the_style_reaches_the_palette() {
        let dir = tempfile::tempdir().expect("temp dir");
        let palette = two_colours();
        let choice = ColorChoice {
            style: Style::Neutral,
            ..ColorChoice::default()
        };
        let got = choice
            .palette(Some((&image(dir.path(), "a.jpg", b"a"), &palette)))
            .expect("palette");
        assert_eq!(got.style, Style::Neutral);
        assert_ne!(got.light.accent, palette.light.accent);
    }
}
