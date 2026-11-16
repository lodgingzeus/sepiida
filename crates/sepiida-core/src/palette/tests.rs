use image::{Rgb as Pixel, RgbImage};

use super::*;

fn solid(r: u8, g: u8, b: u8) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_pixel(256, 144, Pixel([r, g, b])))
}

#[test]
fn palette_from_the_source_colour_matches_the_image_palette() {
    // The palette cache stores only the source colour and rebuilds the
    // rest from it, which is only correct if this holds.
    let from_image = Palette::from_image(&solid(200, 111, 201));
    assert_eq!(
        Palette::from_parts(from_image.candidates, from_image.lightness),
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
fn colourless_images_keep_their_own_colour_as_source() {
    // Material scoring skips all of these and would pick its blue
    // fallback (#4285f4).
    for (r, g, b) in [(0, 0, 0), (128, 128, 128), (250, 250, 250), (20, 22, 24)] {
        let source = Palette::from_image(&solid(r, g, b)).source;
        let close = |want: u8, got: u8| want.abs_diff(got) <= 4;
        assert!(
            close(r, source.r) && close(g, source.g) && close(b, source.b),
            "({r}, {g}, {b}) gave {source}"
        );
    }
}

#[test]
fn a_dark_photo_with_a_small_colourful_area_keeps_that_colour() {
    let mut image = RgbImage::from_pixel(100, 100, Pixel([15, 15, 18]));
    for x in 0..30 {
        for y in 0..30 {
            image.put_pixel(x, y, Pixel([200, 60, 40]));
        }
    }
    let source = Palette::from_image(&DynamicImage::ImageRgb8(image)).source;
    assert!(source.r > source.g && source.r > source.b, "got {source}");
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

fn chroma(c: Color) -> f64 {
    material_colors::hct::Hct::new(c.into()).get_chroma()
}

/// Quarters of four clearly different colours, the largest first.
fn four_colours() -> DynamicImage {
    let mut image = RgbImage::new(100, 100);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = match (x < 60, y < 60) {
            (true, true) => Pixel([200, 60, 40]),
            (false, true) => Pixel([40, 90, 200]),
            (true, false) => Pixel([40, 170, 70]),
            (false, false) => Pixel([220, 190, 40]),
        };
    }
    DynamicImage::ImageRgb8(image)
}

#[test]
fn a_colourful_image_offers_several_distinct_candidates() {
    let p = Palette::from_image(&four_colours());
    let candidates = p.candidates.as_slice();
    assert!(candidates.len() >= 2, "got {candidates:?}");
    assert_eq!(candidates[0], p.source, "the best candidate is the source");
    for (i, a) in candidates.iter().enumerate() {
        assert!(!candidates[i + 1..].contains(a), "repeat in {candidates:?}");
    }
}

#[test]
fn grey_images_and_plain_colours_offer_one_candidate() {
    assert_eq!(
        Palette::from_image(&solid(128, 128, 128))
            .candidates
            .as_slice()
            .len(),
        1
    );
    let color = Color::new(40, 180, 60);
    assert_eq!(Palette::from_color(color).candidates.as_slice(), &[color]);
}

#[test]
fn candidates_skip_repeats_and_stop_at_the_limit() {
    let c = |n| Color::new(n, n, n);
    let list = Candidates::from_colors([c(1), c(2), c(1), c(3), c(4), c(5)]).expect("not empty");
    assert_eq!(list.as_slice(), &[c(1), c(2), c(3), c(4)]);
    assert_eq!(list.best(), c(1));
    assert_eq!(Candidates::from_colors([]), None);
}

#[test]
fn the_default_style_is_soft_and_keeps_the_old_palette() {
    let p = Palette::from_image(&solid(200, 111, 201));
    assert_eq!(p.style, Style::Soft);
    assert_eq!(p.with_choice(p.source, Style::Soft), p);
}

#[test]
fn styles_order_the_accent_colourfulness() {
    let p = Palette::from_color(Color::new(200, 60, 40));
    let accent = |style| chroma(p.with_choice(p.source, style).light.accent);
    let (neutral, soft, vivid) = (
        accent(Style::Neutral),
        accent(Style::Soft),
        accent(Style::Vivid),
    );
    assert!(
        neutral < soft && soft < vivid,
        "neutral {neutral}, soft {soft}, vivid {vivid}"
    );
}

#[test]
fn faithful_keeps_the_source_hue() {
    let source = Color::new(40, 90, 200);
    let p = Palette::from_color(source).with_choice(source, Style::Faithful);
    let hue = |c: Color| material_colors::hct::Hct::new(c.into()).get_hue();
    assert!((hue(p.light.accent) - hue(source)).abs() < 10.0);
}

#[test]
fn a_choice_keeps_the_wallpaper_candidates_and_lightness() {
    let p = Palette::from_image(&four_colours());
    let other = Color::new(10, 200, 220);
    let chosen = p.with_choice(other, Style::Vivid);
    assert_eq!((chosen.source, chosen.style), (other, Style::Vivid));
    assert_eq!(chosen.candidates, p.candidates);
    assert_eq!(chosen.lightness, p.lightness);
}
