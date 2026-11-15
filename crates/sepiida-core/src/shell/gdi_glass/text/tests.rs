use super::*;

fn run(x: i32, y: i32, align: u32, options: u32) -> Run {
    Run {
        x,
        y,
        options,
        align,
        width: 40,
        height: 16,
        ascent: 12,
    }
}

#[test]
fn top_left_text_starts_at_the_point() {
    assert_eq!(
        text_rect(&run(10, 20, 0, 0), None),
        Some(Rect::new(10, 20, 50, 36))
    );
}

#[test]
fn alignment_moves_the_text() {
    // TA_BASELINE | TA_CENTER.
    assert_eq!(
        text_rect(&run(100, 50, TA_BASELINE | TA_CENTER, 0), None),
        Some(Rect::new(80, 38, 120, 54))
    );
    // TA_BOTTOM | TA_RIGHT.
    assert_eq!(
        text_rect(&run(100, 50, TA_BOTTOM | TA_RIGHT, 0), None),
        Some(Rect::new(60, 34, 100, 50))
    );
}

#[test]
fn clipping_shrinks_and_opaque_grows() {
    let rect = Rect::new(0, 0, 30, 30);
    assert_eq!(
        text_rect(&run(10, 10, 0, ETO_CLIPPED), Some(rect)),
        Some(Rect::new(10, 10, 30, 26))
    );
    assert_eq!(
        text_rect(&run(10, 10, 0, ETO_OPAQUE), Some(rect)),
        Some(Rect::new(0, 0, 50, 30))
    );
    // Clipped away entirely: nothing to draw.
    assert_eq!(text_rect(&run(100, 100, 0, ETO_CLIPPED), Some(rect)), None);
}

#[test]
fn empty_text_draws_nothing() {
    let mut empty = run(0, 0, 0, 0);
    empty.width = 0;
    assert_eq!(text_rect(&empty, None), None);
}

#[test]
fn extreme_values_saturate_instead_of_overflowing() {
    let mut far = run(i32::MIN, i32::MAX, TA_BOTTOM | TA_RIGHT, 0);
    far.width = i32::MAX;
    far.height = i32::MAX;
    let _ = text_rect(
        &far,
        Some(Rect::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX)),
    );
    assert_eq!(
        extent_with_advances(1, 1, &[i32::MAX, i32::MIN, i32::MAX, i32::MIN], true),
        (i32::MAX, i32::MAX)
    );
}

#[test]
fn advances_widen_the_extent() {
    assert_eq!(extent_with_advances(10, 16, &[8, 8, 8], false), (24, 16));
    assert_eq!(extent_with_advances(30, 16, &[8, 8, 8], false), (30, 16));
    // ETO_PDY: x and y pairs; the y offset adds to the height.
    assert_eq!(extent_with_advances(0, 16, &[8, -2, 8, -3], true), (16, 21));
    // An odd count with ETO_PDY reads the last x alone.
    assert_eq!(extent_with_advances(0, 16, &[8, 1, 8], true), (16, 17));
}

#[test]
fn rect_union_ignores_empty_rects() {
    let a = Rect::new(0, 0, 10, 10);
    assert_eq!(a.union(&Rect::default()), a);
    assert_eq!(Rect::default().union(&a), a);
    assert_eq!(a.union(&Rect::new(5, 5, 20, 8)), Rect::new(0, 0, 20, 10));
    assert_eq!(a.intersect(&Rect::new(20, 20, 30, 30)), Rect::default());
}

#[test]
fn both_text_tables_boost_edges_and_dark_text_more() {
    let light = alpha_table(0x00ff_ffff);
    let dark = alpha_table(0);
    assert_eq!((light[0], light[255]), (0, 255));
    assert!(light[64] > 64, "antialiased edges are brightened");
    assert_eq!((dark[0], dark[255]), (0, 255));
    assert!(dark[64] > light[64], "dark text edges are boosted more");
    assert!(light.windows(2).all(|w| w[0] <= w[1]));
    assert!(dark.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn the_mask_becomes_premultiplied_text_colour() {
    // Two rows of three pixels with a stride of four: black, half, white.
    let mut pixels = [
        0,
        0x0080_8080,
        0x00ff_ffff,
        0x1234_5678, //
        0x00ff_ffff,
        0,
        0,
        0x1234_5678,
    ];
    // Light yellow text (COLORREF 0x00BBGGRR).
    colorize_mask(&mut pixels, 3, 2, 4, 0x0080_ffff);
    assert_eq!(pixels[0], 0, "no coverage stays transparent");
    let full = pixels[2];
    assert_eq!(full >> 24, 255);
    // Premultiplied by 255/256, as the mod does: red, green, blue.
    assert_eq!((full >> 16) & 0xff, 254);
    assert_eq!((full >> 8) & 0xff, 254);
    assert_eq!(full & 0xff, 127);
    let half = pixels[1];
    assert!((half >> 24) > 0x80 && (half >> 24) < 255);
    assert_eq!(pixels[4], full);
    // Padding beyond the width is not touched.
    assert_eq!(pixels[3], 0x1234_5678);
    assert_eq!(pixels[7], 0x1234_5678);
}

#[test]
fn a_short_buffer_is_not_overrun() {
    let mut pixels = [0x00ff_ffff; 5];
    colorize_mask(&mut pixels, 4, 3, 4, 0);
    assert!(pixels.iter().all(|&p| p >> 24 == 0xff));
    colorize_mask(&mut pixels, 9, 9, 0, 0);
}

#[test]
fn draw_text_draws_in_its_rect_or_its_extent_within_the_clip() {
    let rect = Rect::new(0, 0, 50, 20);
    assert_eq!(draw_text_area(rect, None, None), Some(rect));
    // DT_NOCLIP: the text may run past the rectangle.
    assert_eq!(
        draw_text_area(rect, Some(Rect::new(0, 0, 80, 20)), None),
        Some(Rect::new(0, 0, 80, 20))
    );
    assert_eq!(
        draw_text_area(rect, None, Some(Rect::new(10, 0, 30, 100))),
        Some(Rect::new(10, 0, 30, 20))
    );
    // Clipped away, or an empty rectangle: nothing visible.
    assert_eq!(
        draw_text_area(rect, None, Some(Rect::new(60, 0, 90, 20))),
        None
    );
    assert_eq!(draw_text_area(Rect::default(), None, None), None);
}
