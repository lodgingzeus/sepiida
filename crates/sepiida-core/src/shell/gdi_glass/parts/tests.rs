use super::*;

#[test]
fn scroll_bar_parts_are_classified_as_the_mod_does() {
    assert_eq!(
        paint_for("ScrollBar", 3, 1),
        Some(Paint::Thumb {
            horizontal: false,
            hot: false
        })
    );
    assert_eq!(
        paint_for("scrollbar", 2, 2),
        Some(Paint::Thumb {
            horizontal: true,
            hot: true
        })
    );
    for track in 4..=7 {
        assert_eq!(paint_for("ScrollBar", track, 1), Some(Paint::Clear));
    }
    assert_eq!(
        paint_for("ScrollBar", 8, 1),
        None,
        "the gripper is left alone"
    );
    assert_eq!(
        paint_for("ScrollBar", 1, 1),
        Some(Paint::Arrow(Direction::Up, ArrowState::Normal))
    );
    assert_eq!(
        paint_for("ScrollBar", 1, 6),
        Some(Paint::Arrow(Direction::Down, ArrowState::Hot))
    );
    assert_eq!(
        paint_for("ScrollBar", 1, 16),
        Some(Paint::Arrow(Direction::Right, ArrowState::Disabled))
    );
    assert_eq!(
        paint_for("ScrollBar", 1, 19),
        Some(Paint::Arrow(Direction::Left, ArrowState::BarHovered))
    );
    assert_eq!(paint_for("ScrollBar", 1, 0), None);
    assert_eq!(paint_for("ScrollBar", 1, 21), None);
}

#[test]
fn header_and_separator_parts() {
    assert_eq!(paint_for("Header", 0, 0), Some(Paint::Clear));
    assert_eq!(paint_for("Header", 1, 1), Some(Paint::Clear));
    assert_eq!(
        paint_for("Header", 1, 2),
        Some(Paint::HeaderItem { hot: true })
    );
    assert_eq!(
        paint_for("Header", 1, 3),
        Some(Paint::HeaderItem { hot: false })
    );
    assert_eq!(paint_for("Header", 1, 4), Some(Paint::Clear));
    assert_eq!(paint_for("Header", 2, 1), None);
    assert_eq!(paint_for("PreviewPane", 3, 0), Some(Paint::Clear));
    assert_eq!(paint_for("PreviewPane", 1, 0), None);
    assert_eq!(paint_for("Button", 1, 1), None);
}

fn alpha(p: u32) -> u32 {
    p >> 24
}

fn premultiplied(bitmap: &Bitmap) -> bool {
    bitmap
        .pixels
        .iter()
        .all(|&p| (0..3).all(|i| (p >> (i * 8)) & 0xff <= alpha(p)))
}

#[test]
fn the_thumb_is_a_rounded_bar_inset_from_its_edges() {
    let thin = thumb(false, false, 96);
    assert_eq!((thin.bitmap.width, thin.bitmap.height), (17, 11));
    assert_eq!(thin.margins, [8, 5, 8, 5]);
    let row = |b: &Bitmap, y: usize| b.pixels[y * b.width..(y + 1) * b.width].to_vec();
    let middle = row(&thin.bitmap, 5);
    // Clear at the sides, solid in the middle at the theme's alpha.
    assert_eq!(alpha(middle[0]), 0);
    assert_eq!(alpha(middle[16]), 0);
    assert_eq!(alpha(middle[8]), 128);
    let hot = thumb(false, true, 96);
    let wide = row(&hot.bitmap, 5)
        .iter()
        .filter(|&&p| alpha(p) > 0)
        .count();
    let narrow = middle.iter().filter(|&&p| alpha(p) > 0).count();
    assert!(wide > narrow, "wider when hovered");
    assert!(premultiplied(&thin.bitmap) && premultiplied(&hot.bitmap));
    // Corners are rounded: the top-left of the bar is partly covered.
    assert!(alpha(thin.bitmap.pixels[6]) < 128);
}

#[test]
fn sizes_follow_the_dpi() {
    let large = thumb(true, false, 192);
    assert_eq!((large.bitmap.width, large.bitmap.height), (40, 34));
    assert_eq!(large.margins, [16, 10, 16, 10]);
    let header = header_item(true, 144);
    assert_eq!(header.bitmap.width, 36);
    assert_eq!(header.margins, [18, 0, 16, 18]);
}

#[test]
fn the_header_shade_is_square_on_top_and_round_below() {
    let shade = header_item(true, 96);
    let px = |x: usize, y: usize| shade.bitmap.pixels[y * 24 + x];
    assert_eq!(alpha(px(0, 0)), 96, "square top corner");
    assert_eq!(alpha(px(0, 23)), 0, "round bottom corner");
    assert_eq!(alpha(px(12, 23)), 96);
    assert_eq!(alpha(header_item(false, 96).bitmap.pixels[0]), 64);
    assert!(premultiplied(&shade.bitmap));
}

#[test]
fn arrows_hide_until_hovered_and_point_their_way() {
    assert_eq!(
        arrow_bitmap(Direction::Up, ArrowState::Normal, 16, 16, 96),
        None
    );
    let up = arrow_bitmap(Direction::Up, ArrowState::Hot, 16, 16, 96).unwrap();
    let down = arrow_bitmap(Direction::Down, ArrowState::Hot, 16, 16, 96).unwrap();
    let covered =
        |b: &Bitmap, y: usize| (0..16).filter(|&x| alpha(b.pixels[y * 16 + x]) > 0).count();
    // The up arrow is narrow at the top, wide at the bottom; down the other way.
    assert!(covered(&up, 6) < covered(&up, 10));
    assert!(covered(&down, 6) > covered(&down, 10));
    let right = arrow_bitmap(Direction::Right, ArrowState::BarHovered, 16, 16, 96).unwrap();
    assert!(right.pixels.iter().any(|&p| alpha(p) > 0));
    assert!(premultiplied(&up) && premultiplied(&right));
}

#[test]
fn nine_grid_keeps_margins_and_stretches_the_middle() {
    let pieces = nine_grid(Rect::new(10, 0, 110, 20), 17, 11, [8, 5, 8, 5]);
    assert_eq!(pieces.len(), 9);
    assert_eq!(pieces[0], (Rect::new(10, 0, 18, 5), Rect::new(0, 0, 8, 5)));
    // The middle piece takes the middle of the source.
    assert_eq!(
        pieces[4],
        (Rect::new(18, 5, 102, 15), Rect::new(8, 5, 9, 6))
    );
    // Smaller than the margins: cut down, no inverted pieces.
    let small = nine_grid(Rect::new(0, 0, 6, 4), 17, 11, [8, 5, 8, 5]);
    assert!(small.iter().all(|(d, s)| !d.is_empty() && !s.is_empty()));
    assert!(nine_grid(Rect::default(), 17, 11, [8, 5, 8, 5]).is_empty());
}

#[test]
fn progress_tracks_become_translucent_and_their_fill_is_left_alone() {
    for part in [1, 2, 11, 12] {
        assert_eq!(paint_for("Progress", part, 1), Some(Paint::Track));
    }
    assert_eq!(paint_for("PROGRESS", 11, 1), Some(Paint::Track));
    // PP_FILL and PP_CHUNK stay uxtheme's.
    assert_eq!(paint_for("Progress", 5, 1), None);
    assert_eq!(paint_for("Progress", 3, 1), None);
    let grid = track(96);
    assert_eq!((grid.bitmap.width, grid.bitmap.height), (12, 12));
    assert_eq!(grid.margins, [3; 4]);
    assert_eq!(alpha(grid.bitmap.pixels[6 * 12 + 6]), 56);
    assert!(alpha(grid.bitmap.pixels[0]) < 56, "rounded corner");
    assert!(premultiplied(&grid.bitmap));
}
