//! Layout a look computes from measured sizes: where the taskbar's glass
//! bar ends so that it stops short of the tray's own pill; the widest a
//! centred dock may grow (LiquidGlass2); how far Start's View button moves
//! up past the pinned list (Down Aero).
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

/// Room LiquidGlass2 keeps between the dock and the tray pill (the tray
/// frame's left margin), on each side.
const DOCK_GAP: f32 = 50.0;
/// The narrowest LiquidGlass2 lets the dock become.
const DOCK_MIN: f32 = 100.0;

/// The dock's `MaxWidth` in a grid `container` wide with a tray pill
/// `tray` wide: LiquidGlass2's `max(containerGridWidth - 2 * (trayWidth +
/// 50), 100)`, so the centred dock never runs under the tray (Windows then
/// moves apps into its overflow). `None` for a reading that makes no sense
/// or an empty container (the theme leaves it unlimited then).
pub fn dock_max_width(container: f32, tray: f32) -> Option<f32> {
    let sane = |n: f32| n.is_finite() && (0.0..=MAX_WIDTH).contains(&n);
    if !(sane(container) && sane(tray)) || container == 0.0 {
        return None;
    }
    Some((container - 2.0 * (tray + DOCK_GAP)).max(DOCK_MIN))
}

/// Down Aero's View button sits on the Recommended row, moved left 5 and
/// up 254 plus the pinned list's height:
/// `TranslateTransform X="-5" Y="{{-254 - pinnedListHeight}}"`. Up 255
/// here: at 254 it sat a pixel below the Recommended half it joins and at
/// 256 a pixel above (laptop, 2026-11-16), so the rims did not meet.
const VIEW_SHIFT_X: f32 = -5.0;
const VIEW_SHIFT_UP: f32 = 255.0;

/// The View button's shift (`x`, `y`) for a pinned list `pinned` high.
/// `None` for a reading that makes no sense.
pub fn view_button_shift(pinned: f32) -> Option<(f32, f32)> {
    (pinned.is_finite() && (0.0..=MAX_WIDTH).contains(&pinned))
        .then_some((VIEW_SHIFT_X, -VIEW_SHIFT_UP - pinned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dock_keeps_clear_of_the_tray_and_never_gets_too_narrow() {
        // A 1920 px screen with a 300 px tray pill.
        assert_eq!(dock_max_width(1920.0, 300.0), Some(1220.0));
        assert_eq!(dock_max_width(500.0, 300.0), Some(100.0));
        assert_eq!(dock_max_width(0.0, 300.0), None);
        assert_eq!(dock_max_width(f32::NAN, 300.0), None);
        assert_eq!(dock_max_width(1920.0, -1.0), None);
    }

    #[test]
    fn the_view_button_moves_up_past_the_pinned_list() {
        assert_eq!(view_button_shift(0.0), Some((-5.0, -255.0)));
        assert_eq!(view_button_shift(312.0), Some((-5.0, -567.0)));
        assert_eq!(view_button_shift(f32::INFINITY), None);
        assert_eq!(view_button_shift(-3.0), None);
    }

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
