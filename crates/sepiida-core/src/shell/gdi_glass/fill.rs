//! Background fills over the glass: which colours are the content
//! background (made transparent), and the navigation pane divider.

use super::{ColorRef, text::Rect};

/// Theme parts whose fill colour is the content background in dark mode
/// (class, part). The part ids have no SDK constants.
pub const DARK_BACKGROUND_PARTS: [(&str, i32); 5] = [
    ("DarkMode::ItemsView", 0),
    ("DarkMode::ExplorerNavPane", 0),
    ("DarkMode::ExplorerStatusBar", 0),
    ("DarkMode::ReadingPane", 1),
    ("DarkMode::ProperTree", 2),
];

/// Opacity of the navigation pane divider over the glass.
pub const DIVIDER_ALPHA: u8 = 96;

/// Whether a fill of `color` paints the content background: the window
/// colour (`COLOR_WINDOW`, light mode) or a dark-mode fill colour.
pub fn is_window_background(color: ColorRef, window: ColorRef, dark_fills: &[ColorRef]) -> bool {
    color == window || dark_fills.contains(&color)
}

/// The rectangle a solid pen of `pen_width` covers drawing the line from
/// `a` to `b`, if it is horizontal: a wide pen is centred on the line, and
/// the last point is not drawn.
pub fn divider_rect(a: (i32, i32), b: (i32, i32), pen_width: i32) -> Option<Rect> {
    if a.1 != b.1 || a.0 == b.0 {
        return None;
    }
    let width = pen_width.max(1);
    let top = a.1.saturating_sub(width / 2);
    Some(Rect::new(
        a.0.min(b.0),
        top,
        a.0.max(b.0),
        top.saturating_add(width),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_colour_and_dark_fills_are_background() {
        let dark = [0x0019_1919, 0x0020_2020];
        assert!(is_window_background(0x00ff_ffff, 0x00ff_ffff, &dark));
        assert!(is_window_background(0x0020_2020, 0x00ff_ffff, &dark));
        assert!(!is_window_background(0x00ff_0000, 0x00ff_ffff, &dark));
        assert!(!is_window_background(0x0019_1919, 0x00ff_ffff, &[]));
    }

    #[test]
    fn a_horizontal_line_covers_its_pen_width() {
        assert_eq!(
            divider_rect((10, 5), (2, 5), 1),
            Some(Rect::new(2, 5, 10, 6))
        );
        assert_eq!(divider_rect((0, 5), (8, 5), 3), Some(Rect::new(0, 4, 8, 7)));
        // A zero-width (cosmetic) pen draws one pixel.
        assert_eq!(divider_rect((0, 5), (8, 5), 0), Some(Rect::new(0, 5, 8, 6)));
    }

    #[test]
    fn other_lines_are_not_dividers() {
        assert_eq!(divider_rect((0, 0), (0, 9), 1), None);
        assert_eq!(divider_rect((0, 0), (9, 1), 1), None);
        assert_eq!(divider_rect((4, 4), (4, 4), 1), None);
    }
}
