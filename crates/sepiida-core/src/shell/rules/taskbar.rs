//! Explorer's taskbar: the background colour rule, the base every look
//! starts from, and the segmented floating look of M5 step 3.

use super::rule::{Effect, GRID_TYPE, Name, Paint, Part, Place, RECTANGLE_TYPE, Role, Rule, look};
use crate::shell::{
    layout::{Value, Visibility},
    measure::bar_margin,
};

/// The root of one taskbar's own tree. The hover flyout also contains a
/// `BackgroundFill` rectangle, but not under this element.
pub const TASKBAR_FRAME_NAME: &str = "TaskbarFrame";

/// The rectangle that paints the taskbar background (Windows 11 23H2 and
/// 24H2: `TaskbarFrame > Grid#RootGrid > TaskbarBackground > Grid >
/// Rectangle#BackgroundFill`). Its `Fill` is a local `AcrylicBrush` (M3
/// round 3), tinted like the other surfaces (M4, 2026-11-14).
pub const TASKBAR_BACKGROUND: Rule = Rule {
    label: "taskbar background",
    type_name: RECTANGLE_TYPE,
    name: Name::Is("BackgroundFill"),
    ancestor: Some(TASKBAR_FRAME_NAME),
    parent: None,
    chain: &[],
    position: None,
    conditions: &[],
    material: None,
    property: "Fill",
    effect: Effect::Color {
        paint: Paint::Tint,
        role: Role::Surface,
    },
};

/// `Rectangle#BackgroundFill`, Windows' own taskbar background.
const TASKBAR_FILL: Place = (
    RECTANGLE_TYPE,
    "BackgroundFill",
    Some(TASKBAR_FRAME_NAME),
    None,
);
/// `Rectangle#BackgroundStroke`, the line along the taskbar's top edge.
const TASKBAR_STROKE: Place = (
    RECTANGLE_TYPE,
    "BackgroundStroke",
    Some(TASKBAR_FRAME_NAME),
    None,
);
/// The unnamed grid that holds the fill and the stroke
/// (`TaskbarBackground#BackgroundControl > Grid`, build 26200). The hover
/// flyout's background is named differently, so it is not selected.
const TASKBAR_BACKGROUND_GRID: Place = (GRID_TYPE, "", None, Some("BackgroundControl"));
/// `StackPanel#SystemTrayFrameGrid`: the tray icons and the clock (a
/// StackPanel on build 26200).
const TRAY: Place = (
    "Windows.UI.Xaml.Controls.StackPanel",
    "SystemTrayFrameGrid",
    None,
    None,
);

/// Space between the buttons and the edge of their pill, and between the
/// pills and the screen edge or each other, in effective pixels.
const PILL_SIDE: f32 = 8.0;
/// Space above and below each pill.
const PILL_INSET: f32 = 4.0;
const PILL_CORNERS: Value = Value::CornerRadius([8.0; 4]);
const COLLAPSED: Value = Value::Visibility(Visibility::Collapsed);
const GLASS: Value = Value::Acrylic(Role::Surface);

/// A rule that marks a taskbar element for the measured layout.
const fn taskbar_measure(
    label: &'static str,
    place: Place,
    property: &'static str,
    part: Part,
) -> Rule {
    Rule {
        effect: Effect::Measure(part),
        ..look(label, place, property, COLLAPSED)
    }
}

/// The glass bar's margin for a tray panel `tray` wide: from the left
/// edge to a gap before the tray's pill. `None` for a bad reading.
pub fn taskbar_bar_margin(tray: f32) -> Option<Value> {
    // The tray pill's own right margin is `PILL_SIDE`.
    bar_margin(tray + PILL_SIDE, PILL_SIDE, PILL_INSET, PILL_SIDE).map(Value::Thickness)
}

/// Windows' taskbar background hidden while the look is on.
const HIDE_FILL: Rule = look(
    "Windows' taskbar background",
    TASKBAR_FILL,
    "Visibility",
    COLLAPSED,
);
const HIDE_STROKE: Rule = look("taskbar edge line", TASKBAR_STROKE, "Visibility", COLLAPSED);

/// What every taskbar look starts from: Windows' background tinted with
/// the palette, and hidden while the look is on. RosePine (`rose_pine`)
/// draws the rest.
pub const TASKBAR_BASE: &[Rule] = &[TASKBAR_BACKGROUND, HIDE_FILL, HIDE_STROKE];

/// The segmented floating taskbar of M5 step 3, not in a rule set since
/// RosePine (kept: the measured layout is built and tested on it): Windows' background hidden; a glass bar of
/// our acrylic behind the buttons, from the left edge to a gap before the
/// tray (its end measured from the tray's width); the tray its own pill.
/// Nothing Windows lays out is moved or narrowed: it sizes the buttons
/// from where they sit, so the buttons stay as Windows places them.
pub const TASKBAR_RULES: &[Rule] = &[
    TASKBAR_BACKGROUND,
    HIDE_FILL,
    HIDE_STROKE,
    look("bar glass", TASKBAR_BACKGROUND_GRID, "Background", GLASS),
    look(
        "bar corners",
        TASKBAR_BACKGROUND_GRID,
        "CornerRadius",
        PILL_CORNERS,
    ),
    taskbar_measure("bar extent", TASKBAR_BACKGROUND_GRID, "Margin", Part::Bar),
    taskbar_measure("tray width", TRAY, "", Part::Tray),
    look("tray pill glass", TRAY, "Background", GLASS),
    look("tray pill corners", TRAY, "CornerRadius", PILL_CORNERS),
    look(
        "tray pill inset",
        TRAY,
        "Margin",
        Value::Thickness([0.0, PILL_INSET, PILL_SIDE, PILL_INSET]),
    ),
    look(
        "tray pill padding",
        TRAY,
        "Padding",
        Value::Thickness([PILL_SIDE / 2.0, 0.0, PILL_SIDE / 2.0, 0.0]),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_background_rule_selects_only_the_fill_rectangle() {
        assert!(TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects(GRID_TYPE, "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "Other"));
    }

    #[test]
    fn every_taskbar_colour_is_a_tint() {
        assert!(TASKBAR_RULES.iter().all(|rule| match rule.effect {
            Effect::Color { paint, .. } => paint == Paint::Tint,
            Effect::Set(_) | Effect::Probe | Effect::Measure(_) => true,
        }));
    }

    #[test]
    fn the_taskbar_has_a_look() {
        assert!(
            TASKBAR_RULES
                .iter()
                .any(|r| matches!(r.effect, Effect::Set(_)))
        );
    }
}
