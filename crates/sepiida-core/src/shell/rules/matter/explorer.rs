//! Matter for File Explorer's XAML (WinUI 3), solid material only.

use crate::shell::framework::Framework;
use crate::shell::rules::{
    Material,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    (
        "accentColor",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" />"),
    ),
    (
        "accentColor2",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" Opacity=\"0.5\" />",
        ),
    ),
];

pub const EXPLORER: Theme = Theme {
    framework: Framework::WinUi,
    material: Some(Material::Solid),
    targets: &[
        (
            "CommandBar#FileExplorerCommandBar",
            &["Background=Transparent", "HorizontalAlignment  = 1"],
        ),
        (
            "Grid#TabContainerGrid > Border#LeftBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "Grid#TabContainerGrid > Border#RightBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        ("TabViewItem", &["Margin=0,0,4,0"]),
        (
            "TabViewItem > Grid#LayoutRoot",
            &["CornerRadius=5", "Margin=2,4,0,4", "Height=29"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot > Canvas",
            &["Visibility=Collapsed"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot > Grid#TabContainer",
            &["Background = Transparent", "BorderThickness = 0"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot@CommonStates",
            &[
                "Background@Selected:= $accentColor2",
                "Background@PointerOverSelected:= $accentColor",
                "Background@PointerOver:= $accentColor2",
                "Background@Normal=$accentColor",
                "Background@PressedSelected:=$accentColor2",
                "Background@Pressed := $accentColor2",
            ],
        ),
        (
            "Grid#TabContainerGrid > Border > Button#AddButton",
            &["Visibility  = 0", "Margin = 0,0,0,3"],
        ),
        (
            "FileExplorerExtensions.CommandBarControl_Wave1 > Grid, Grid#CommandBarControlRootGrid",
            &["Background=Transparent", "BorderThickness = 0"],
        ),
        ("Grid#NavigationBarControlGrid", &["Background=Transparent"]),
        (
            "Grid#PART_LayoutRoot",
            &[
                "Background :=<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" Opacity=\"0.4\" />",
                "CornerRadius = 6",
                "BorderThickness = 0",
            ],
        ),
        (
            "FileExplorerExtensions.CommandBarControl_Wave1, FileExplorerExtensions.CommandBarControl",
            &["Margin = 0,-5,0,0"],
        ),
        (
            "AutoSuggestBox#FileExplorerSearchBox > Grid#LayoutRoot > TextBox > Grid@CommonStates > Border#BorderElement",
            &[
                "Background :=<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" Opacity=\"0.4\" />",
                "CornerRadius = 6",
                "BorderThickness = 0",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Border#ScrollDecreaseButtonContainer",
            &["Margin = 0,0,0,3"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Border#ScrollIncreaseButtonContainer",
            &["Margin = 0,0,0,3"],
        ),
    ],
    vars: VARS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::windhawk::build;

    #[test]
    fn every_look_rule_is_for_the_solid_material_and_only_tabs_wait() {
        let built = build(&EXPLORER);
        assert!(!built.rules.is_empty());
        for rule in &built.rules {
            assert_eq!(rule.material, Some(Material::Solid), "{}", rule.label);
        }
        let left_out: Vec<&str> = built.skipped.iter().map(|s| s.target).collect();
        assert_eq!(
            left_out,
            [
                "TabViewItem > Grid#LayoutRoot@CommonStates",
                "AutoSuggestBox#FileExplorerSearchBox > Grid#LayoutRoot > TextBox > Grid@CommonStates > Border#BorderElement",
            ]
        );
    }

    #[test]
    fn no_button_is_hidden() {
        for (target, styles) in EXPLORER.targets {
            assert!(!target.contains("AppBar"), "{target}");
            assert!(
                !styles.iter().any(|s| s.replace(' ', "") == "Visibility=1"),
                "{target}"
            );
        }
    }
}
