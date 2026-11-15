//! The Start menu (StartMenuExperienceHost).

use super::{
    rule::{BORDER_TYPE, Name, Part, Rule, measured},
    sandboxed::{overlay, tinted},
};

/// Build 26100: `Grid#RootGrid > Grid#RootContent > Border#AcrylicBorder`,
/// with overlays nearby.
pub const START_RULES: &[Rule] = &[
    tinted("Start background", BORDER_TYPE, "AcrylicBorder"),
    overlay("Start acrylic overlay", "AcrylicOverlay"),
    overlay("Start colour overlay", "BackgroundColorOverlay"),
    overlay("Start full-bleed fallback", "FallbackFullBleedBackground"),
    // Down Aero's `ActualHeight=>pinnedListHeight` and the View button's
    // `TranslateTransform Y="{{-254 - pinnedListHeight}}"` (`down_aero`).
    measured(
        "pinned list height",
        "StartMenu.PinnedList",
        Name::Any,
        "",
        Part::PinnedList,
    ),
    measured(
        "View button place",
        "Microsoft.UI.Xaml.Controls.DropDownButton",
        Name::Any,
        "RenderTransform",
        Part::ViewButton,
    ),
];
