//! The flyouts: notification centre, calendar and toasts
//! (ShellExperienceHost), quick settings and media controls (ShellHost).
//! Both processes use this one set, as Windhawk's notification centre
//! styler does; each finds only its own elements. The look is WindowGlass
//! (`window_glass`); the overlays below are only logged.

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
        tree.add(4, 3, GRID, "");
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
        let mut handles: Vec<u64> = rules_for(Host::ShellExperience)
            .iter()
            .filter(|r| r.property == property && r.effect == effect)
            .flat_map(|r| tree.matching(r))
            .collect();
        handles.sort_unstable();
        handles
    }

    #[test]
    fn cards_get_the_raised_colour_and_round_corners() {
        let card = Effect::Color {
            paint: Paint::Solid,
            role: Role::SurfaceRaised,
        };
        assert_eq!(matched("Background", card), [5]);
        // WindowGlass: panel 20, items 16, plating 12.
        let corners = |text| matched("CornerRadius", Effect::Set(Value::Text(text)));
        assert_eq!(corners("20"), [2]);
        assert_eq!(corners("16"), [3]);
        assert_eq!(corners("12"), [5]);
    }

    #[test]
    fn the_panel_and_only_the_focus_sections_own_grid_get_glass() {
        let glass = Effect::Set(Value::Blur {
            amount: 5.0,
            role: Role::Surface,
            opacity: 0.7,
        });
        assert_eq!(matched("Background", glass), [2, 7]);
    }

    #[test]
    fn our_slider_thumb_replaces_the_themes_glass() {
        let mut tree = ElementTree::new();
        tree.add(
            1,
            0,
            "Windows.UI.Xaml.Controls.Primitives.Thumb",
            "HorizontalThumb",
        );
        tree.add(2, 1, BORDER, "");
        let looks: Vec<Effect> = rules_for(Host::ShellHost)
            .iter()
            .filter(|r| r.property == "Background" && matches!(r.effect, Effect::Set(_)))
            .filter(|r| !tree.matching(r).is_empty())
            .map(|r| r.effect)
            .collect();
        assert_eq!(looks.len(), 1);
        assert!(
            matches!(looks[0], Effect::Set(Value::Xaml(x)) if x.contains("SystemChromeWhiteColor"))
        );
    }

    #[test]
    fn both_flyout_hosts_share_the_set() {
        assert_eq!(
            rules_for(Host::ShellHost).as_ptr(),
            rules_for(Host::ShellExperience).as_ptr()
        );
    }
}
