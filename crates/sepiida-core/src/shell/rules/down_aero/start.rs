//! Down Aero for the redesigned Start. Target and style lines are copied
//! unchanged, with these exceptions:
//! - the panel (`Border#AcrylicBorder`) takes our panel glass
//!   (`rules::glass`) and the inner overlay is clear while it is on; the
//!   theme's other neutral brushes are kept as written, as variables,
//!   since we never load inline `:=` XAML;
//! - the Recommended button and the View button form one split pill, as
//!   written (Recommended the left half, View the right), but View takes
//!   Recommended's clear fill instead of its own solid one, and neither
//!   half has a rim (the user's choice: the theme's hover rim needs visual
//!   states, so a rim could only be there all the time);
//! - captures (`=>`) and `{{…}}` expressions are not supported yet:
//!   `showMoreSuggestionsVisible` is taken as 0 (Visible, the default), so
//!   `Visibility={{…}}` is dropped and `{{…*n}}` becomes 0; the View
//!   button's move by the pinned list's height is a measured layout of
//!   ours (`rules::start`, `measure::view_button_shift`) in place of its
//!   `RenderTransform` line;
//! - visual states (`@CommonStates`, `@PointerOver`) are not supported
//!   yet: those entries keep their normal look as plain setters, without
//!   the hover colours;
//! - the "Show all" button's own half-pill entry is left out (a
//!   `[Property=Value]` on an ancestor); it gets the header buttons' look.

use crate::shell::framework::Framework;
use crate::shell::rules::{
    glass::PANEL_GLASS,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    // The panel's `<AcrylicBrush TintColor="{ThemeResource
    // CardStrokeColorDefaultSolid}" TintLuminosityOpacity=".5" />`: our
    // panel glass instead, as on File Explorer.
    ("Panel", PANEL_GLASS),
    // The inner overlay's opaque acrylic (luminosity 1) would hide the
    // glass: clear while the glass is on, as written for the solid look.
    (
        "Overlay",
        Var::Material {
            glass: &Var::Transparent,
            solid: &Var::Xaml(
                "<AcrylicBrush TintColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" FallbackColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" TintOpacity=\"0\" TintLuminosityOpacity=\"1\" Opacity=\"1\"/>",
            ),
        },
    ),
    (
        "Card",
        Var::Xaml(
            "<AcrylicBrush TintColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" FallbackColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" TintOpacity=\"0\" TintLuminosityOpacity=\".5\" Opacity=\"1\"/>",
        ),
    ),
    (
        "CardStrong",
        Var::Xaml(
            "<AcrylicBrush TintColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" FallbackColor=\"{ThemeResource CardStrokeColorDefaultSolid}\" TintOpacity=\"0\" TintLuminosityOpacity=\"1\" Opacity=\"1\"/>",
        ),
    ),
    (
        "PowerRing",
        Var::Xaml(
            "<AcrylicBrush TintColor=\"{ThemeResource SurfaceStrokeColorDefault}\" FallbackColor=\"{ThemeResource SurfaceStrokeColorDefault}\" TintOpacity=\"0\" TintLuminosityOpacity=\".1\" Opacity=\"1\"/>",
        ),
    ),
    (
        "PowerRingHover",
        Var::Xaml(
            "<AcrylicBrush TintColor=\"{ThemeResource SystemAccentColor}\" FallbackColor=\"{ThemeResource SystemAccentColor}\" TintOpacity=\".8\" TintLuminosityOpacity=\".5\" Opacity=\"1\"/>",
        ),
    ),
    (
        "PillHigh",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeAltHighColor}\" Opacity=\".8\"/>",
        ),
    ),
    (
        "PillFill",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAltMediumLowColor}\" Opacity=\"1\" />",
        ),
    ),
    (
        "PillClear",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAltMediumLowColor}\" Opacity=\"0\" />",
        ),
    ),
    (
        "PillHover",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemBaseLowColor}\" Opacity=\".7\" />",
        ),
    ),
    (
        "PillHoverStrong",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemBaseLowColor}\" Opacity=\"1\" />"),
    ),
    (
        "PillBack",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeMediumLowColor}\" Opacity=\"1\"/>",
        ),
    ),
];

pub const START: Theme = Theme {
    framework: Framework::System,
    material: None,
    targets: &[
        ("Grid#FrameRoot", &["MaxHeight=520"]),
        ("TextBlock#ZoomedOutHeading", &["Visibility=Collapsed"]),
        (
            "Windows.UI.Xaml.Controls.Grid#TopLevelSuggestionsListHeader",
            &["Height=0"],
        ),
        ("Button#ShowMoreSuggestionsButton", &["Margin=0,-77,147,0"]),
        (
            "Windows.UI.Xaml.Controls.Grid#NoTopLevelSuggestionsText",
            &["Height=0"],
        ),
        (
            "Button#ShowMoreSuggestionsButton > Grid > ContentPresenter > StackPanel > TextBlock",
            &["Text=Recommended", "Visibility=Visible"],
        ),
        ("Border#StartDropShadow", &["CornerRadius=30"]),
        ("StartMenu.SearchBoxToggleButton", &["Visibility=Collapsed"]),
        (
            "Border#AcrylicBorder",
            &["CornerRadius=30", "Background:=$Panel"],
        ),
        (
            "Border#AcrylicOverlay",
            &[
                "CornerRadius=30",
                "Background:=$Overlay",
                "Height=430",
                "Margin=0,-65,0,0",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AllAppsRoot",
            &["Margin=0,-90,0,90"],
        ),
        (
            "Grid#MainContent",
            &["Grid.Row=0", "VerticalAlignment=0", "MinHeight=Auto"],
        ),
        (
            "StartDocked.AppListView#NavigationPanePlacesListView > Windows.UI.Xaml.Controls.Border",
            &["Background:=$Card", "CornerRadius=18", "Margin=0,0,15,0"],
        ),
        (
            "StartDocked.NavigationPaneButton#PowerButton > Windows.UI.Xaml.Controls.Grid > Windows.UI.Xaml.Controls.Border#BackgroundBorder",
            &[
                "Background:=$CardStrong",
                "BorderBrush:=$PowerRing",
                "CornerRadius=30",
                "BorderThickness=5",
                "Margin=-7",
            ],
        ),
        (
            "StartDocked.NavigationPaneButton#UserTileButton > Grid > Border#BackgroundBorder",
            &["Background:=$Card", "CornerRadius=18"],
        ),
        (
            "Button#ShowMoreSuggestionsButton > Grid > Border",
            &[
                "Background:=$PillClear",
                "BorderBrush=Transparent",
                "Padding=10,5",
                "Margin=0,0,-2,0",
                "CornerRadius=15,0,0,15",
                "BorderThickness=2,2,0,2",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#HideMoreSuggestionsButton",
            &["Background:=$PillBack", "CornerRadius=15"],
        ),
        (
            "StartDocked.NavigationPaneView > Windows.UI.Xaml.Controls.Grid#RootPanel",
            &["Grid.Row=2"],
        ),
        ("Windows.UI.Xaml.Controls.Frame", &["Margin=0,-65,0,0"]),
        ("Grid#MainMenu", &["MaxWidth=650"]),
        ("TextBlock#AllListHeadingText", &["Margin=63,-184,12,0"]),
        (
            "Windows.UI.Xaml.Controls.GridView#RecommendedList",
            &["Visibility=Collapsed"],
        ),
        (
            "Windows.UI.Xaml.Controls.GridView#AllAppsGrid > Border > Windows.UI.Xaml.Controls.ScrollViewer > Border > Grid > Windows.UI.Xaml.Controls.ScrollContentPresenter > Windows.UI.Xaml.Controls.ItemsPresenter > Windows.UI.Xaml.Controls.ItemsWrapGrid",
            &["Margin=45,-180,45,0"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.DropDownButton > Grid",
            &[
                "BorderBrush=Transparent",
                "Background:=$PillClear",
                "BorderThickness=0,2,2,2",
                "CornerRadius=0,15,15,0",
                "Height=32",
            ],
        ),
        ("StartMenu.PinnedList", &["Margin=0,20,12,180"]),
        ("Grid#TopLevelSuggestionsRoot", &["Grid.Row=1"]),
        (
            "Microsoft.UI.Xaml.Controls.DropDownButton",
            &["MaxWidth=100"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.DropDownButton > Grid > ContentPresenter > TextBlock",
            &["Margin=8,0,8,0", "Text=View"],
        ),
        ("StartMenu.CategoryControl", &["Margin=15,0,-15,0"]),
        (
            "Grid#TopLevelHeader > Grid > Button",
            &[
                "Margin=-430,0,430,0",
                "Height=32",
                "CornerRadius=15",
                "BorderThickness=0,2,2,2",
            ],
        ),
        ("Border#RightCompanionDropShadow", &["CornerRadius=30"]),
        (
            "Grid#CompanionRoot > Grid#MainContent > Border#AcrylicOverlay",
            &["Margin=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.ScrollBar",
            &["Visibility=Collapsed"],
        ),
        (
            "Grid#TopLevelHeader > Grid > Button > Grid > Border",
            &[
                "Background:=$PillFill",
                "BorderBrush:=$PillHigh",
                "BorderThickness=2",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.ToggleButton",
            &["Margin=12,7,-12,-7"],
        ),
        (
            "Grid#MainMenu > Grid#MainContent > Grid",
            &["Canvas.ZIndex=1"],
        ),
    ],
    vars: VARS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::windhawk::build;

    #[test]
    fn no_entry_is_left_out() {
        let built = build(&START);
        let left_out: Vec<(&str, &str)> = built
            .skipped
            .iter()
            .map(|s| (s.target, s.reason.as_str()))
            .collect();
        assert_eq!(left_out, [], "{left_out:#?}");
        assert!(built.rules.len() > 40, "{}", built.rules.len());
    }

    #[test]
    fn no_inline_xaml_captures_or_expressions_remain() {
        for (target, styles) in START.targets {
            for style in *styles {
                assert!(!style.contains(":=<"), "{target}: {style}");
                assert!(!style.contains("=>"), "{target}: {style}");
                assert!(!style.contains("{{"), "{target}: {style}");
                assert!(!style.contains('@'), "{target}: {style}");
            }
        }
    }
}
