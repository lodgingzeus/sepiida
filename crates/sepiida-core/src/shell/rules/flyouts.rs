//! The flyouts: notification centre, calendar and toasts
//! (ShellExperienceHost), quick settings and media controls (ShellHost).
//! Both processes use this one set, as Windhawk's notification centre
//! styler does; each finds only its own elements. The look is
//! BetterControl11 with our materials (`better_control`) and WindowGlass
//! for the popups (`window_glass`); the overlays below are only logged.

use super::{rule::Rule, sandboxed::overlay};

/// Elements that may cover a surface; only logged.
pub const FLYOUT_OVERLAYS: &[Rule] = &[
    overlay("toast highlight", "ToastAcrylicHightlightBorder"),
    overlay("calendar background layer", "BackgroundLayer"),
    overlay("quick settings root border", "RootGridBorder"),
];

#[cfg(test)]
mod tests {
    use crate::shell::{
        framework::Framework,
        hosts::Host,
        layout::Value,
        rules::{Effect, Paint, Role, rules_for},
        tree::ElementTree,
    };

    const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
    const BORDER: &str = "Windows.UI.Xaml.Controls.Border";

    /// Parts of the notification centre as dumped on build 26200.
    fn notification_centre() -> ElementTree {
        let mut tree = ElementTree::new();
        tree.add(1, 0, "ActionCenter.NotificationCenterPage", "");
        tree.add(2, 1, GRID, "NotificationCenterGrid");
        tree.add(3, 2, "ActionCenter.FlexibleItemView", "");
        tree.add(9, 3, GRID, "MainGrid");
        tree.add(10, 9, GRID, "ItemGrid");
        tree.add(4, 10, GRID, "");
        tree.add(5, 4, BORDER, "ItemOpaquePlating");
        tree.add(
            6,
            1,
            "ActionCenter.FocusSessionControl",
            "FocusSessionControl",
        );
        tree.add(7, 6, GRID, "FocusGrid");
        // A FocusGrid elsewhere is left alone.
        tree.add(8, 1, GRID, "FocusGrid");
        tree
    }

    fn matched(property: &str, effect: Effect) -> Vec<u64> {
        let tree = notification_centre();
        let mut handles: Vec<u64> = rules_for(Host::ShellExperience, Framework::System)
            .iter()
            .filter(|r| r.property == property && r.effect == effect)
            .flat_map(|r| tree.matching(r))
            .collect();
        handles.sort_unstable();
        handles
    }

    #[test]
    fn cards_get_the_raised_colour_and_the_themes_corners() {
        let card = Effect::Color {
            paint: Paint::Solid,
            role: Role::SurfaceRaised,
        };
        assert_eq!(matched("Background", card), [5]);
        // BetterControl11: the panel joins the calendar below it.
        let corners = |text| matched("CornerRadius", Effect::Set(Value::Text(text)));
        assert_eq!(corners("8,8,0,0"), [2]);
        assert_eq!(corners("6"), [5]);
    }

    #[test]
    fn only_the_panel_gets_glass() {
        let glass = Effect::Set(Value::Blur {
            amount: crate::shell::rules::glass::PANEL_BLUR,
            role: Role::Surface,
            opacity: crate::shell::rules::glass::PANEL_TINT,
        });
        assert_eq!(matched("Background", glass), [2]);
    }

    #[test]
    fn both_flyout_hosts_share_the_set() {
        assert_eq!(
            rules_for(Host::ShellHost, Framework::System).as_ptr(),
            rules_for(Host::ShellExperience, Framework::System).as_ptr()
        );
    }
}
