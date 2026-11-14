//! What the hook DLL styles, as data: which elements (type, `x:Name` and a
//! required ancestor), which property, how, and which palette colour goes
//! there. New surfaces are added as rules here rather than as code in the
//! DLL. Each shell process has its own rule set (`rules_for`).

use super::hosts::Host;

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
    /// Replace the value with a `SolidColorBrush` of the colour.
    Solid,
    /// Keep Windows' own `AcrylicBrush` and set its tint to the colour.
    /// Values that are not an `AcrylicBrush` are left alone.
    Tint,
    /// Change nothing; log the property's value chain. For elements that
    /// may cover a styled surface (overlays), to learn whether they do.
    LogOnly,
}

/// One kind of element to style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// The property that receives the colour.
    pub property: &'static str,
    pub paint: Paint,
    pub role: Role,
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
/// Rectangle#BackgroundFill`).
pub const TASKBAR_BACKGROUND: Rule = Rule {
    label: "taskbar background",
    type_name: RECTANGLE_TYPE,
    name: "BackgroundFill",
    ancestor: Some(TASKBAR_FRAME_NAME),
    property: "Fill",
    paint: Paint::Solid,
    role: Role::Surface,
};

/// Everything styled in Explorer's taskbar.
pub const TASKBAR_RULES: &[Rule] = &[TASKBAR_BACKGROUND];

/// A surface background in a sandboxed host: tinted acrylic.
const fn tinted(label: &'static str, type_name: &'static str, name: &'static str) -> Rule {
    Rule {
        label,
        type_name,
        name,
        ancestor: None,
        property: "Background",
        paint: Paint::Tint,
        role: Role::Surface,
    }
}

/// A border that may lie over a surface; only logged.
const fn overlay(label: &'static str, name: &'static str) -> Rule {
    Rule {
        label,
        type_name: BORDER_TYPE,
        name,
        ancestor: None,
        property: "Background",
        paint: Paint::LogOnly,
        role: Role::Surface,
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

    #[test]
    fn every_host_has_rules_and_something_to_paint() {
        for host in Host::ALL {
            let rules = rules_for(host);
            assert!(rules.iter().any(|r| r.paint != Paint::LogOnly), "{host:?}");
        }
    }

    #[test]
    fn the_taskbar_stays_solid_and_sandboxed_surfaces_are_tinted() {
        assert!(TASKBAR_RULES.iter().all(|r| r.paint == Paint::Solid));
        for host in [Host::StartMenu, Host::ShellHost, Host::ShellExperience] {
            assert!(
                rules_for(host).iter().all(|r| r.paint != Paint::Solid),
                "{host:?}"
            );
        }
    }

    #[test]
    fn no_two_rules_of_a_host_select_the_same_element() {
        for host in Host::ALL {
            let rules = rules_for(host);
            for (i, a) in rules.iter().enumerate() {
                for b in &rules[i + 1..] {
                    assert!(
                        !(a.type_name == b.type_name && a.name == b.name),
                        "{host:?}: {} and {}",
                        a.label,
                        b.label
                    );
                }
            }
        }
    }
}
