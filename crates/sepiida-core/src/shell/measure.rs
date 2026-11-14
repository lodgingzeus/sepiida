//! Layout a look computes from measured sizes: where the taskbar's glass
//! bar ends so that it stops short of the tray's own pill.
//!
//! XAML's property chain reports sizes as strings (`ActualWidth` "322",
//! `DesiredSize` "821.333x48") but no positions (build 26200). Moving the
//! taskbar's buttons is no option: Windows sizes them from where they sit
//! on screen, so a moved or narrowed frame makes it fold the search box and
//! push apps into its overflow (seen 2026-11-14). So only the glass, which
//! Windows does not lay out, is placed from a measured width.

/// The largest width taken for real; anything above is a bad reading.
const MAX_WIDTH: f32 = 100_000.0;

/// A length as the chain writes it, e.g. "1280" or "45.3333".
pub fn parse_length(text: &str) -> Option<f32> {
    let n: f32 = text.trim().parse().ok()?;
    (n.is_finite() && (0.0..=MAX_WIDTH).contains(&n)).then_some(n)
}

/// The width of a size as the chain writes it, e.g. "821.333x48".
pub fn parse_size_width(text: &str) -> Option<f32> {
    let (width, height) = text.split_once('x')?;
    parse_length(height)?;
    parse_length(width)
}

/// The glass bar's margin (`[left, top, right, bottom]`): `side` from the
/// left edge, `inset` above and below, ending `gap` before the tray, which
/// takes `tray` (its width with its own margin) at the right. `None` for
/// readings that make no sense.
pub fn bar_margin(tray: f32, side: f32, inset: f32, gap: f32) -> Option<[f32; 4]> {
    let sane = |n: f32| n.is_finite() && (0.0..=MAX_WIDTH).contains(&n);
    if !(sane(tray) && sane(side) && sane(inset) && sane(gap)) {
        return None;
    }
    Some([side, inset, tray + gap, inset])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_strings_are_parsed_and_nonsense_refused() {
        assert_eq!(parse_length("1280"), Some(1280.0));
        assert_eq!(parse_length(" 45.3333 "), Some(45.3333));
        assert_eq!(parse_length("-1"), None);
        assert_eq!(parse_length("NaN"), None);
        assert_eq!(parse_length("inf"), None);
        assert_eq!(parse_length(""), None);
        assert_eq!(parse_length("1e9"), None);
        assert_eq!(parse_size_width("821.333x48"), Some(821.333));
        assert_eq!(parse_size_width("16x8"), Some(16.0));
        assert_eq!(parse_size_width("821.333"), None);
        assert_eq!(parse_size_width("x48"), None);
        assert_eq!(parse_size_width("10xNaN"), None);
    }

    #[test]
    fn the_bar_ends_a_gap_before_the_tray() {
        // The laptop on 2026-11-14: tray panel 330 with padding, margin 8.
        assert_eq!(
            bar_margin(338.0, 8.0, 4.0, 8.0),
            Some([8.0, 4.0, 346.0, 4.0])
        );
        assert_eq!(bar_margin(0.0, 8.0, 4.0, 8.0), Some([8.0, 4.0, 8.0, 4.0]));
    }

    #[test]
    fn bad_readings_give_no_margin() {
        assert_eq!(bar_margin(f32::NAN, 8.0, 4.0, 8.0), None);
        assert_eq!(bar_margin(-1.0, 8.0, 4.0, 8.0), None);
        assert_eq!(bar_margin(1e9, 8.0, 4.0, 8.0), None);
    }
}
