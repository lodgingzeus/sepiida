//! What the hook DLL styles, as data: which elements (type, `x:Name` and a
//! required ancestor), which property, and what it gets: a palette colour,
//! or a layout value while the look is on. New surfaces are added as rules
//! here rather than as code in the DLL. Each shell process has its own rule
//! set (`rules_for`).

use super::{
    hosts::Host,
    layout::{Value, Visibility},
    measure::bar_margin,
};

/// A palette colour sepiida.exe sends to the DLL. The DLL paints every
/// target of a role with that role's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Role {
    /// Large background areas: the taskbar, Start, quick settings, the
    /// notification centre.
    Surface = 1,
    /// Highlights; not used by any rule yet.
    Accent = 2,
}

impl Role {
    pub const ALL: [Self; 2] = [Self::Surface, Self::Accent];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.id() == id)
    }
}

/// How a rule's property gets the role's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Replace the value with a `SolidColorBrush` of the colour. No rule
    /// uses it since the taskbar moved to tint; kept as the fallback a
    /// surface without acrylic can be switched to.
    Solid,
    /// Keep Windows' own `AcrylicBrush` and set its tint to the colour.
    /// Values that are not an `AcrylicBrush` are left alone.
    Tint,
    /// Change nothing; log the property's value chain. For elements that
    /// may cover a styled surface (overlays), to learn whether they do.
    LogOnly,
}

/// What a rule does to its property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// The palette colour of `role`, applied the `paint` way. Always on
    /// while styling.
    Color { paint: Paint, role: Role },
    /// A layout value, set only while the look is on and put back when it
    /// is turned off.
    Set(Value),
    /// Changes nothing: logs the element's layout properties (size,
    /// offset) whenever the look is switched, to learn the geometry a
    /// look must fit. `property` is unused.
    Probe,
    /// Part of the taskbar's measured layout (`measure`), while the look
    /// is on.
    Measure(Part),
}

/// The elements the taskbar's measured layout reads or sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// The glass bar behind the buttons: its `Margin` is computed.
    Bar,
    /// The tray panel: its `ActualWidth` is the tray's width.
    Tray,
}

/// One kind of element to style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// For logs, e.g. "taskbar background".
    pub label: &'static str,
    /// The type XAML Diagnostics reports for the element.
    pub type_name: &'static str,
    /// The element's `x:Name`.
    pub name: &'static str,
    /// The `x:Name` one of its ancestors must have; tells apart elements
    /// that share a type and name in different parts of the shell.
    pub ancestor: Option<&'static str>,
    /// The `x:Name` its direct parent must have; for an element whose name
    /// is common (`RootGrid`) and whose place is known exactly.
    pub parent: Option<&'static str>,
    /// The property the rule changes.
    pub property: &'static str,
    pub effect: Effect,
}

impl Rule {
    /// Whether an element of this type and name is a candidate; the
    /// ancestor is checked separately, since it may arrive later.
    pub fn selects(&self, type_name: &str, name: &str) -> bool {
        self.type_name == type_name && self.name == name
    }
}

/// Position of a rule in the rule set it came from.
pub type RuleId = usize;

/// The type XAML Diagnostics reports for rectangles.
pub const RECTANGLE_TYPE: &str = "Windows.UI.Xaml.Shapes.Rectangle";
pub const BORDER_TYPE: &str = "Windows.UI.Xaml.Controls.Border";
pub const GRID_TYPE: &str = "Windows.UI.Xaml.Controls.Grid";
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
    name: "BackgroundFill",
    ancestor: Some(TASKBAR_FRAME_NAME),
    parent: None,
    property: "Fill",
    effect: Effect::Color {
        paint: Paint::Tint,
        role: Role::Surface,
    },
};

/// Where a taskbar look rule's element is: type, `x:Name`, the name of a
/// required ancestor and of a required direct parent.
type Place = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
);

/// A look rule on an element of the taskbar.
const fn taskbar_look(
    label: &'static str,
    (type_name, name, ancestor, parent): Place,
    property: &'static str,
    value: Value,
) -> Rule {
    Rule {
        label,
        type_name,
        name,
        ancestor,
        parent,
        property,
        effect: Effect::Set(value),
    }
}

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
        ..taskbar_look(label, place, property, COLLAPSED)
    }
}

/// The glass bar's margin for a tray panel `tray` wide: from the left
/// edge to a gap before the tray's pill. `None` for a bad reading.
pub fn taskbar_bar_margin(tray: f32) -> Option<Value> {
    // The tray pill's own right margin is `PILL_SIDE`.
    bar_margin(tray + PILL_SIDE, PILL_SIDE, PILL_INSET, PILL_SIDE).map(Value::Thickness)
}

/// Everything styled in Explorer's taskbar. The look (M5 step 3) is a
/// segmented floating taskbar: Windows' background hidden; a glass bar of
/// our acrylic behind the buttons, from the left edge to a gap before the
/// tray (its end measured from the tray's width); the tray its own pill.
/// Nothing Windows lays out is moved or narrowed: it sizes the buttons
/// from where they sit, so the buttons stay as Windows places them.
pub const TASKBAR_RULES: &[Rule] = &[
    TASKBAR_BACKGROUND,
    taskbar_look(
        "Windows' taskbar background",
        TASKBAR_FILL,
        "Visibility",
        COLLAPSED,
    ),
    taskbar_look("taskbar edge line", TASKBAR_STROKE, "Visibility", COLLAPSED),
    taskbar_look("bar glass", TASKBAR_BACKGROUND_GRID, "Background", GLASS),
    taskbar_look(
        "bar corners",
        TASKBAR_BACKGROUND_GRID,
        "CornerRadius",
        PILL_CORNERS,
    ),
    taskbar_measure("bar extent", TASKBAR_BACKGROUND_GRID, "Margin", Part::Bar),
    taskbar_measure("tray width", TRAY, "", Part::Tray),
    taskbar_look("tray pill glass", TRAY, "Background", GLASS),
    taskbar_look("tray pill corners", TRAY, "CornerRadius", PILL_CORNERS),
    taskbar_look(
        "tray pill inset",
        TRAY,
        "Margin",
        Value::Thickness([0.0, PILL_INSET, PILL_SIDE, PILL_INSET]),
    ),
    taskbar_look(
        "tray pill padding",
        TRAY,
        "Padding",
        Value::Thickness([PILL_SIDE / 2.0, 0.0, PILL_SIDE / 2.0, 0.0]),
    ),
];

/// A surface background in a sandboxed host: tinted acrylic.
const fn tinted(label: &'static str, type_name: &'static str, name: &'static str) -> Rule {
    Rule {
        label,
        type_name,
        name,
        ancestor: None,
        parent: None,
        property: "Background",
        effect: Effect::Color {
            paint: Paint::Tint,
            role: Role::Surface,
        },
    }
}

/// A border that may lie over a surface; only logged.
const fn overlay(label: &'static str, name: &'static str) -> Rule {
    Rule {
        label,
        type_name: BORDER_TYPE,
        name,
        ancestor: None,
        parent: None,
        property: "Background",
        effect: Effect::Color {
            paint: Paint::LogOnly,
            role: Role::Surface,
        },
    }
}

/// The Start menu (StartMenuExperienceHost, build 26100: `Grid#RootGrid >
/// Grid#RootContent > Border#AcrylicBorder`, with overlays nearby).
pub const START_RULES: &[Rule] = &[
    tinted("Start background", BORDER_TYPE, "AcrylicBorder"),
    overlay("Start acrylic overlay", "AcrylicOverlay"),
    overlay("Start colour overlay", "BackgroundColorOverlay"),
    overlay("Start full-bleed fallback", "FallbackFullBleedBackground"),
];

/// Quick settings and media controls (ShellHost, build 26100: both under
/// `Grid#RootGrid > Grid#RootContent`).
pub const SHELL_HOST_RULES: &[Rule] = &[
    tinted(
        "quick settings background",
        GRID_TYPE,
        "ControlCenterRegion",
    ),
    tinted(
        "media controls background",
        GRID_TYPE,
        "MediaTransportControlsRegion",
    ),
    overlay("quick settings root border", "RootGridBorder"),
];

/// Notification centre, calendar and toasts (ShellExperienceHost, build
/// 26100).
pub const SHELL_EXPERIENCE_RULES: &[Rule] = &[
    tinted(
        "notification centre background",
        GRID_TYPE,
        "NotificationCenterGrid",
    ),
    tinted("calendar background", GRID_TYPE, "CalendarCenterGrid"),
    tinted("toast background", BORDER_TYPE, "ToastBackgroundBorder"),
    overlay("toast highlight", "ToastAcrylicHightlightBorder"),
    overlay("calendar background layer", "BackgroundLayer"),
    overlay("notification card plating", "ItemOpaquePlating"),
];

/// The rule set for elements in `host`.
pub const fn rules_for(host: Host) -> &'static [Rule] {
    match host {
        Host::Explorer => TASKBAR_RULES,
        Host::StartMenu => START_RULES,
        Host::ShellHost => SHELL_HOST_RULES,
        Host::ShellExperience => SHELL_EXPERIENCE_RULES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ids_round_trip_and_unknown_ids_are_rejected() {
        for role in Role::ALL {
            assert_eq!(Role::from_id(role.id()), Some(role));
        }
        assert_eq!(Role::from_id(0), None);
        assert_eq!(Role::from_id(3), None);
        assert_eq!(Role::from_id(u8::MAX), None);
    }

    #[test]
    fn a_rule_selects_on_type_and_name() {
        assert!(TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects(BORDER_TYPE, "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "Other"));
    }

    fn paint(rule: &Rule) -> Option<Paint> {
        match rule.effect {
            Effect::Color { paint, .. } => Some(paint),
            Effect::Set(_) | Effect::Probe | Effect::Measure(_) => None,
        }
    }

    #[test]
    fn every_host_has_rules_and_something_to_paint() {
        for host in Host::ALL {
            let rules = rules_for(host);
            assert!(
                rules
                    .iter()
                    .any(|r| paint(r).is_some_and(|p| p != Paint::LogOnly)),
                "{host:?}"
            );
        }
    }

    #[test]
    fn every_surface_is_tinted_and_none_painted_solid() {
        assert!(
            TASKBAR_RULES
                .iter()
                .filter_map(paint)
                .all(|p| p == Paint::Tint)
        );
        for host in Host::ALL {
            assert!(
                rules_for(host)
                    .iter()
                    .all(|r| paint(r) != Some(Paint::Solid)),
                "{host:?}"
            );
        }
    }

    #[test]
    fn no_two_rules_of_a_host_change_the_same_property_of_an_element() {
        for host in Host::ALL {
            let rules = rules_for(host);
            for (i, a) in rules.iter().enumerate() {
                for b in &rules[i + 1..] {
                    assert!(
                        !(a.type_name == b.type_name
                            && a.name == b.name
                            && a.property == b.property),
                        "{host:?}: {} and {}",
                        a.label,
                        b.label
                    );
                }
            }
        }
    }

    #[test]
    fn every_look_value_is_valid_and_the_taskbar_has_a_look() {
        let mut looks = 0;
        for host in Host::ALL {
            for rule in rules_for(host) {
                if let Effect::Set(value) = rule.effect {
                    assert!(value.is_valid(), "{}", rule.label);
                    looks += 1;
                }
            }
        }
        assert!(
            TASKBAR_RULES
                .iter()
                .any(|r| matches!(r.effect, Effect::Set(_)))
        );
        assert!(looks > 0);
    }
}
