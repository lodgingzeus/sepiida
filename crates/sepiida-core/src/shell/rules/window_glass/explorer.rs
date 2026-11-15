//! WindowGlass for File Explorer's WinUI 3 parts, from Windhawk's Windows 11
//! File Explorer Styler v1.7.2 (commit `12b5a5c`): a transparent tab strip,
//! address bar and command bar over the window's backdrop, pill tabs, a
//! centred command bar without labels, the Home and Gallery pages as an
//! opaque rounded card, and opaque menus and tooltips (WinUI menus cannot
//! be translucent). All target and style lines are copied unchanged. Our
//! variables: the menus and tooltips (the theme's blur and near-opaque
//! acrylic) take the palette's surface colour as a solid brush, since the
//! blur brush and our acrylic tint are system-XAML only so far; the border
//! gradient, the content card and the accent buttons stay as written
//! (their `ThemeResource`s follow Windows, whose accent sepiida sets).
//! Entries for elements this build lacks (`NavigationViewItemPresenter`,
//! `Grid#RootContainer`, `RootCommandSearchGrid`) simply never match; the
//! two `@VisualState` entries are skipped until visual states work.

use crate::shell::framework::Framework;
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    // `<WindhawkBlur BlurAmount="15" TintColor="#15323232"/>`.
    ("Background", Var::Solid(Role::Surface)),
    // `<AcrylicBrush TintColor="{ThemeResource SystemChromeMediumLowColor}"
    // TintOpacity="0.9" Opacity="0.96"/>`.
    ("AcrylicBackground", Var::Solid(Role::Surface)),
    (
        "BorderBrush",
        Var::Xaml(
            "<LinearGradientBrush StartPoint=\"0,0\" EndPoint=\"0,1\"><GradientStop Color=\"{ThemeResource SystemChromeHighColor}\" Offset=\"0.0\" /><GradientStop Color=\"{ThemeResource SystemChromeLowColor}\" Offset=\"0.15\" /><GradientStop Color=\"{ThemeResource SystemChromeHighColor}\" Offset=\"0.95\" /></LinearGradientBrush>",
        ),
    ),
    ("BorderThickness", Var::Text("0.3,1,0.3,0.3")),
    (
        "ButtonBackground",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemAccentColor}\" Opacity=\"1\" />"),
    ),
    (
        "ButtonBorder",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight3}\" Opacity=\"1\" />",
        ),
    ),
    ("CornerRadius", Var::Text("8")),
    (
        "MainContentBG",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeAltHighColor}\" Opacity=\"1\" />",
        ),
    ),
];

pub const EXPLORER: Theme = Theme {
    framework: Framework::WinUi,
    targets: &[
        (
            "Microsoft.UI.Xaml.Controls.Grid#PART_LayoutRoot",
            &[
                "Background=Transparent",
                "RenderTransform:=<TranslateTransform X=\"0\"/>",
            ],
        ),
        (
            "FileExplorerExtensions.FirstCrumbStackPanelControl#FirstCrumbStackPanel",
            &["Visibility=Collapsed"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#RootCommandSearchGrid > Windows.UI.Xaml.Controls.Border#BorderElement",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Primitives.NavigationViewItemPresenter#NavigationViewItemPresenter > Microsoft.UI.Xaml.Controls.Grid#LayoutRoot",
            &[
                "BorderThickness=$BorderThickness",
                "Background:=$ButtonBackground",
                "BorderBrush:=$ButtonBorder",
            ],
        ),
        (
            "FileExplorerExtensions.CommandBarControl_Wave1 > Grid, Grid#CommandBarControlRootGrid",
            &["Background=Transparent", "BorderBrush=Transparent"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar#FileExplorerCommandBar",
            &[
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"0\" />",
                "HorizontalAlignment=Center",
                "Margin=-4",
                "Padding=10",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar#FileExplorerSecondaryCommandBar",
            &[
                "RenderTransform:=<TranslateTransform X=\"Auto\" />",
                "HorizontalAlignment=Center",
                "Margin=-4",
                "Padding=10",
                "Visibility=Collapsed",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar#FileExplorerCommandBar > Microsoft.UI.Xaml.Controls.Grid#LayoutRoot > Microsoft.UI.Xaml.Controls.Grid#ContentRoot",
            &[
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "BorderBrush=Transparent",
                "Background=Transparent",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar#FileExplorerSecondaryCommandBar > Microsoft.UI.Xaml.Controls.Grid#LayoutRoot > Microsoft.UI.Xaml.Controls.Grid#ContentRoot",
            &[
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "Background=#10808080",
                "Visibility=Collapsed",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#NavigationBarControlGrid",
            &[
                "Background=Transparent",
                "BorderBrush=Transparent",
                "ColumnDefinitions:=<ColumnDefinitionCollection><ColumnDefinition Width=\"Auto\"/><ColumnDefinition Width=\"*\"/><ColumnDefinition Width=\"430\"/></ColumnDefinitionCollection>",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#HomeViewRootGrid",
            &[
                "BorderBrush:=$MainContentBG",
                "CornerRadius=8",
                "BorderThickness=0",
                "Margin=0,0,8,8",
                "Background:=$MainContentBG",
            ],
        ),
        (
            "FileExplorerExtensions.GalleryViewControl#GalleryViewControl > Grid",
            &[
                "BorderBrush:=$MainContentBG",
                "CornerRadius=8",
                "BorderThickness=0",
                "Margin=0,0,8,8",
                "Background:=$MainContentBG",
            ],
        ),
        (
            "FileExplorerExtensions.GalleryViewControl#GalleryViewControl > Grid > Grid#GalleryRootGrid",
            &["Background:=$MainContentBG"],
        ),
        ("ToolTip", &["Background:=$AcrylicBackground"]),
        (
            "Grid#TabContainerGrid > Border#LeftBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "Grid#TabContainerGrid > Border#RightBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot",
            &["CornerRadius=8", "Margin=5", "Height=35"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot > Canvas",
            &["Visibility=Collapsed"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot > Grid#TabContainer",
            &["Background=Transparent", "BorderBrush=Transparent"],
        ),
        (
            "TabViewItem > Grid#LayoutRoot@CommonStates",
            &[
                "Background@Selected:=<SolidColorBrush Color=\"#808080\" Opacity=\"0.10\"/>",
                "Background@PointerOverSelected:=<SolidColorBrush Color=\"#808080\" Opacity=\"0.10\"/>",
                "Background@PointerOver:=<AcrylicBrush TintColor=\"Transparent\" Opacity=\"0.13\"/>",
                "Background@Normal:=<AcrylicBrush TintColor=\"Transparent\" Opacity=\"0.05\"/>",
                "Background@PressedSelected:=<SolidColorBrush Color=\"#808080\" Opacity=\"0.10\"/>",
            ],
        ),
        (
            "Grid#TabContainerGrid > Border#LeftBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "Grid#TabContainerGrid > Border#RightBottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Border#BottomBorderLine",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Shapes.Path#LeftRadiusRenderArc",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Shapes.Path#RightRadiusRenderArc",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#TabContainer",
            &["Visibility=Visible"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Viewbox#IconBox",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Primitives.CommandBarFlyoutCommandBar > Grid#LayoutRoot > Grid#OuterContentRoot > Grid#ContentRoot > Grid#PrimaryItemsRoot",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Primitives.CommandBarFlyoutCommandBar > Grid#LayoutRoot@ExpansionStates > Grid#OuterContentRoot > Grid#ContentRoot > Microsoft.UI.Xaml.Controls.Primitives.Popup#OverflowPopup",
            &[
                "VerticalOffset@ExpandedUp=-5",
                "VerticalOffset@ExpandedDown=5",
            ],
        ),
        (
            "Grid#OuterOverflowContentRootV2",
            &["CornerRadius=$CornerRadius"],
        ),
        (
            "Grid#OuterOverflowContentRootV2 > Grid#OverflowContentRoot > CommandBarOverflowPresenter#SecondaryItemsControl > Grid#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "MenuFlyoutPresenter > Border",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Grid#SecondaryItemsControlShadowWrapper > CommandBarOverflowPresenter#SecondaryItemsControl > Grid#LayoutRoot",
            &[
                "Background:=$AcrylicBackground",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.AutoSuggestBox#FileExplorerSearchBox > Microsoft.UI.Xaml.Controls.Grid#LayoutRoot > Microsoft.UI.Xaml.Controls.TextBox#TextBox",
            &[
                "CornerRadius=$CornerRadius",
                "Margin=0,0,180,0",
                "Background=Transparent",
                "BorderBrush=Transparent",
            ],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#FileExplorerAddressBarGrid",
            &["MaxWidth=750", "CornerRadius=$CornerRadius"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.AutoSuggestBox#PART_AutoSuggestBox > Microsoft.UI.Xaml.Controls.Grid#LayoutRoot > Microsoft.UI.Xaml.Controls.TextBox#TextBox",
            &["CornerRadius=$CornerRadius"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar#NavigationCommands",
            &["Margin=180,0,0,0"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#RootContainer",
            &["Background=Transparent"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Border > Microsoft.UI.Xaml.Controls.Button#AddButton",
            &["RenderTransform:=<TranslateTransform Y=\"-6\" />"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.CommandBar > * > Microsoft.UI.Xaml.Controls.TextBlock#TextLabel",
            &["Visibility=Collapsed"],
        ),
        (
            "Microsoft.UI.Xaml.Controls.Grid#SubItemChevronPanel > Microsoft.UI.Xaml.Controls.FontIcon#SubItemChevron",
            &["RenderTransform:=<TranslateTransform X=\"-5\" Y=\"12\" />"],
        ),
    ],
    vars: VARS,
};
