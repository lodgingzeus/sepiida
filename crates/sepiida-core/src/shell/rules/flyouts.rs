//! The flyouts: notification centre, calendar and toasts
//! (ShellExperienceHost), quick settings and media controls (ShellHost).
//! Both processes use this one set, as Windhawk's notification centre
//! styler does; each finds only its own elements.
//!
//! `TINTED_GLASS` is the TintedGlass theme by TheRealCisWhiteMale from
//! Windhawk's windows-11-notification-center-styler v1.8 by m417z
//! (https://github.com/ramensoftware/windhawk-mods, commit `4197cf6`,
//! GPL-3.0), its target and style lines copied unchanged. Only its
//! variables are ours (`VARS`): its blur keeps its strength, tinted with
//! the palette. `ADDITIONS` holds our own lines on top.

use super::{
    rule::Rule,
    sandboxed::overlay,
    windhawk::{Theme, Var},
};
use crate::shell::rules::Role;

/// What the themes' variables stand for here.
const VARS: &[(&str, Var)] = &[
    // TintedGlass: `<WindhawkBlur BlurAmount="18" TintColor="#80000000"/>`:
    // the same blur, the palette's surface colour at the same 50 %. With
    // the look off, Windows' acrylic tinted with the palette.
    (
        "Base",
        Var::Blur {
            amount: 18.0,
            role: Role::Surface,
            opacity: 0.5,
        },
    ),
    // TintedGlass: `<WindhawkBlur BlurAmount="18" TintColor="#1AFFFFFF"/>`,
    // a light veil for menus.
    (
        "Overlay",
        Var::Blur {
            amount: 18.0,
            role: Role::Surface,
            opacity: 0.1,
        },
    ),
    ("Radius", Var::Text("14")),
    ("Transparent", Var::Transparent),
    ("thumbnailImageSize", Var::Text("300")),
    // Ours: opaque notification cards in the raised surface colour.
    ("Card", Var::Solid(Role::SurfaceRaised)),
];

pub const TINTED_GLASS: Theme = Theme {
    targets: &[
        (
            "Grid#NotificationCenterGrid",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
            ],
        ),
        (
            "Grid#CalendarCenterGrid",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
            ],
        ),
        (
            "ScrollViewer#CalendarControlScrollViewer",
            &["BorderThickness=0,0,0,0", "Background:=$Transparent"],
        ),
        (
            "Border#CalendarHeaderMinimizedOverlay",
            &["Background:=$Transparent", "BorderThickness=0,0,0,0"],
        ),
        (
            "ActionCenter.FocusSessionControl#FocusSessionControl > Grid#FocusGrid",
            &["Background:=$Transparent", "BorderThickness=0,0,0,0"],
        ),
        (
            "MenuFlyoutPresenter > Border",
            &[
                "Background:=$Overlay",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
                "Padding=2,4,2,4",
            ],
        ),
        (
            "Border#JumpListRestyledAcrylic",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
                "Margin=-2,-2,-2,-2",
            ],
        ),
        (
            "Grid#ControlCenterRegion",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
            ],
        ),
        (
            "Grid#L1Grid > Border",
            &["Background:=$Transparent", "BorderThickness=0,0,0,0"],
        ),
        (
            "Grid#MediaTransportControlsRegion",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
            ],
        ),
        (
            "Grid#MediaTransportControlsRoot",
            &["Background:=$Transparent"],
        ),
        (
            "ContentPresenter#PageContent",
            &["Background:=$Transparent"],
        ),
        (
            "ContentPresenter#PageContent > Grid > Border",
            &["Background:=$Transparent"],
        ),
        (
            "QuickActions.ControlCenter.AccessibleWindow#PageWindow > ContentPresenter > Grid#FullScreenPageRoot",
            &["Background:=$Transparent"],
        ),
        (
            "QuickActions.ControlCenter.AccessibleWindow#PageWindow > ContentPresenter > Grid#FullScreenPageRoot > ContentPresenter#PageHeader",
            &["Background:=$Transparent"],
        ),
        ("ScrollViewer#ListContent", &["Background:=$Transparent"]),
        (
            "ActionCenter.FlexibleToastView#FlexibleNormalToastView",
            &["Background:=$Transparent"],
        ),
        (
            "Border#ToastBackgroundBorder, Border#ToastBackgroundBorder2",
            &[
                "Background:=$Base",
                "BorderThickness=0,0,0,0",
                "CornerRadius=$Radius",
            ],
        ),
        (
            "JumpViewUI.SystemItemListViewItem > Grid#LayoutRoot > Border#BackgroundBorder",
            &["Background:=$Transparent", "CornerRadius=$Radius"],
        ),
        (
            "JumpViewUI.JumpListListViewItem > Grid#LayoutRoot > Border#BackgroundBorder",
            &["CornerRadius=$Radius"],
        ),
        ("ActionCenter.FlexibleItemView", &["CornerRadius=$Radius"]),
        ("Grid#MediaTransportControlsRegion", &["Height=Auto"]),
        (
            "Grid#ThumbnailImage",
            &[
                "Width=$thumbnailImageSize",
                "Height=$thumbnailImageSize",
                "HorizontalAlignment=Center",
                "VerticalAlignment=Top",
                "Grid.Column=1",
                "Margin=0,2,0,45",
            ],
        ),
        ("Grid#ThumbnailImage > Border", &["CornerRadius=$Radius"]),
        (
            "StackPanel#PrimaryAndSecondaryTextContainer",
            &["VerticalAlignment=Bottom", "Grid.Column=0"],
        ),
        (
            "StackPanel#PrimaryAndSecondaryTextContainer > TextBlock#TitleText",
            &["TextAlignment=Center"],
        ),
        (
            "StackPanel#PrimaryAndSecondaryTextContainer > TextBlock#SubtitleText",
            &["TextAlignment=Center"],
        ),
        (
            "ContentControl#TogglesGroup > ContentPresenter > ControlCenter.PaginatedGridView > Grid",
            &["BorderThickness=0,0,0,0"],
        ),
        ("Grid#FooterGrid", &["BorderThickness=0,0,0,0"]),
    ],
    vars: VARS,
};

/// Our lines on top of the theme: notification cards follow the palette
/// (TintedGlass leaves Windows' grey cards).
pub const ADDITIONS: Theme = Theme {
    targets: &[(
        "Border#ItemOpaquePlating",
        &["Background:=$Card", "CornerRadius=$Radius"],
    )],
    vars: VARS,
};

/// Elements that may cover a surface; only logged.
pub const FLYOUT_OVERLAYS: &[Rule] = &[
    overlay("toast highlight", "ToastAcrylicHightlightBorder"),
    overlay("calendar background layer", "BackgroundLayer"),
    overlay("quick settings root border", "RootGridBorder"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{
        hosts::Host,
        layout::Value,
        rules::{Effect, Paint, rules_for},
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
        // TintedGlass rounds the panel and the items, we the plating.
        assert_eq!(
            matched("CornerRadius", Effect::Set(Value::Text("14"))),
            [2, 3, 5]
        );
    }

    #[test]
    fn only_the_focus_sections_own_grid_is_cleared() {
        assert_eq!(matched("Background", Effect::Set(Value::Transparent)), [7]);
    }

    #[test]
    fn both_flyout_hosts_share_the_set() {
        assert_eq!(
            rules_for(Host::ShellHost).as_ptr(),
            rules_for(Host::ShellExperience).as_ptr()
        );
    }
}
