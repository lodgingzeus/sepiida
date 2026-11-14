//! WindowGlass for Start: one rounded glass panel, the pinned grid and the
//! companion pane as separate glass cards (both Start layouts: the
//! `StartMenu.*` and the older `StartDocked.*` elements). Left out:
//! `FontFamily` lines (Morganite, vivo Sans) and the search box's
//! placeholder text (the author's own).

use super::edge::{PANEL_EDGE, PANEL_EDGE_THICKNESS};
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    // `$Glass`: `<WindhawkBlur BlurAmount="5" TintColor="{ThemeResource
    // SystemChromeMediumColor}" TintOpacity="0.7" />`, tinted with the
    // palette's surface colour instead.
    (
        "Background",
        Var::Blur {
            amount: 5.0,
            role: Role::Surface,
            opacity: 0.7,
        },
    ),
    ("BorderBrush", PANEL_EDGE),
    (
        "ClockBG",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemAccentColor}\" Opacity=\"1\"/>"),
    ),
    ("BorderThickness", PANEL_EDGE_THICKNESS),
    ("CornerRadius", Var::Text("35")),
    ("SearchBoxRadius", Var::Text("20")),
    (
        "ElementBG",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeAltHighColor}\" Opacity=\"0.3\" />",
        ),
    ),
    ("ElementBorderThickness", Var::Text("0.3,0.3,0.3,1")),
    ("ElementCornerRadius", Var::Text("25")),
    (
        "ElementBorderBrush",
        Var::Xaml(
            "<LinearGradientBrush StartPoint=\"0,0\" EndPoint=\"0,1\"><GradientStop Color=\"#50808080\" Offset=\"1\" /><GradientStop Color=\"#50606060\" Offset=\"0.15\" /></LinearGradientBrush>",
        ),
    ),
    ("HoverCornerRadius", Var::Text("15")),
];

pub const START: Theme = Theme {
    targets: &[
        (
            "StackPanel#TimeAndDatePanel",
            &[
                "VerticalAlignment=Top",
                "HorizontalAlignment=Center",
                "RenderTransform:=<TranslateTransform X=\"0\" />",
            ],
        ),
        (
            "StackPanel#TimePanel > TextBlock#Time",
            &[
                "HorizontalAlignment:=Center",
                "RenderTransform:=<TransformGroup><TranslateTransform X=\"-30\" Y=\"-10\" /><ScaleTransform ScaleX=\"3.3\" ScaleY=\"6\" /></TransformGroup>",
                "Foreground:=$ClockBG",
            ],
        ),
        (
            "StackPanel#TimeAndDatePanel > TextBlock#Date",
            &[
                "HorizontalAlignment=Center",
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"-110\" />",
                "Foreground:=$ClockBG",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#WidgetFrameGrid",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#WidgetCanvasPanel",
            &[
                "HorizontalAlignment=Center",
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"50\" />",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#MediaTransportControls",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#MediaControlsContainer",
            &[
                "Visibility=Visible",
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"-250\" />",
                "Margin=0,0,0,0",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#RootPanel > Windows.UI.Xaml.Controls.Grid#RootGrid > Windows.UI.Xaml.Controls.Grid#RootContent",
            &["Margin=-20,-20,-20,0"],
        ),
        ("StartDocked.StartSizingFrame", &["Width=860"]),
        (
            "Windows.UI.Xaml.Controls.Border#RootGridDropShadow",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#RightCompanionDropShadow",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#StartDropShadow",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#DropShadowDismissTarget",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
                "Margin=2",
                "Padding=0",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#RootContent > Windows.UI.Xaml.Controls.Border#AcrylicBorder",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius=$ElementCornerRadius",
                "Margin=0,60,0,10",
                "Visibility=1",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#AcrylicOverlay",
            &["Visibility=1"],
        ),
        (
            "StartDocked.SearchBoxToggleButton#StartMenuSearchBox",
            &["Width=650", "Height=50", "Margin=0,-15,0,0"],
        ),
        (
            "StartDocked.SearchBoxToggleButton#StartMenuSearchBox > Windows.UI.Xaml.Controls.Grid > Windows.UI.Xaml.Controls.Border#BorderElement",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#TopLevelRoot > Windows.UI.Xaml.Controls.Grid",
            &["Visibility=1"],
        ),
        (
            "StartDocked.NavigationPaneView#NavigationPane",
            &[
                "Width=550",
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"10\" />",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#ShowAllAppsButton",
            &["Visibility=1"],
        ),
        (
            "StartMenu.PinnedList#StartMenuPinnedList",
            &["Margin=0", "Height=280"],
        ),
        (
            "StartMenu.PinnedList#StartMenuPinnedList > Windows.UI.Xaml.Controls.Grid#Root",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#UndockedRoot",
            &[
                "Visibility=0",
                "Width=650",
                "Margin=0,-130,0,230",
                "Canvas.ZIndex=1",
                "MaxHeight:=340",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AllAppsRoot",
            &[
                "Visibility=0",
                "Margin=-1600,190,115,-100",
                "MaxHeight=330",
                "Background:=$ElementBG",
                "CornerRadius=$ElementCornerRadius",
                "Width=650",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#CloseAllAppsButton",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.TextBlock#AllAppsHeading",
            &["Visibility=1"],
        ),
        (
            "StartDocked.AllAppsPane#AllAppsPanel",
            &["Margin=-20,-20,20,20"],
        ),
        (
            "StartDocked.StartMenuCompanion#RightCompanion > Windows.UI.Xaml.Controls.Grid#CompanionRoot > Windows.UI.Xaml.Controls.Border#AcrylicBorder",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius:=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#CompanionRoot > Windows.UI.Xaml.Controls.Grid#MainContent > Windows.UI.Xaml.Controls.Grid#ActionsBar > Windows.UI.Xaml.Controls.Button#PrimaryActionBarButton > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$ElementCornerRadius",
                "Height=40",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#ActionsBar > Windows.UI.Xaml.Controls.Button#ActionBarOverflowButton",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$ElementCornerRadius",
                "Height=40",
            ],
        ),
        (
            "StartDocked.StartMenuCompanion#RightCompanion > Windows.UI.Xaml.Controls.Grid#CompanionRoot",
            &["Height=730", "Margin=0,-10,0,-10", "Padding=10,0,-2,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#OverflowFlyoutBackgroundBorder",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.MenuFlyoutPresenter > Windows.UI.Xaml.Controls.Border",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#HoverFlyoutGrid > Windows.UI.Xaml.Controls.Border#HoverFlyoutBackground",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Cortana.UI.Views.TaskbarSearchPage > Windows.UI.Xaml.Controls.Grid#RootGrid > Windows.UI.Xaml.Controls.Grid#OuterBorderGrid",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#LayerBorder",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#AccentLayerBorder",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#dropshadow",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#AppBorder, Windows.UI.Xaml.Controls.Border#AccentAppBorder",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.ToolTip > Windows.UI.Xaml.Controls.ContentPresenter#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=15",
            ],
        ),
        (
            "StartMenu.FolderModal#StartFolderModal > Windows.UI.Xaml.Controls.Grid#Root",
            &[
                "MaxHeight:=420",
                "MaxWidth:=420",
                "Height=Auto",
                "Width=Auto",
            ],
        ),
        (
            "StartMenu.FolderModal#StartFolderModal > Windows.UI.Xaml.Controls.Grid#Root > Windows.UI.Xaml.Controls.ContentControl#ContentControl > Windows.UI.Xaml.Controls.ContentPresenter > StartMenu.UniversalTileContainer#UniversalTileContainer > Windows.UI.Xaml.Controls.Grid#GridViewContainer",
            &["Width=400", "Height=400"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#Root > Windows.UI.Xaml.Controls.Border",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        ("StartMenu.ExpandedFolderList", &["Margin=0,30,0,-120"]),
        (
            "Windows.UI.Xaml.Controls.Grid#MainMenu > Windows.UI.Xaml.Controls.Border#AcrylicBorder",
            &["Visibility=1"],
        ),
        (
            "StartMenu.SearchBoxToggleButton#SearchBoxToggleButton",
            &["Height=50", "Margin=-20,20,-20,-20", "Width=400"],
        ),
        (
            "StartMenu.SearchBoxToggleButton#SearchBoxToggleButton > Windows.UI.Xaml.Controls.Grid > Windows.UI.Xaml.Controls.Border#BorderElement",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius:=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.ToggleButton#ShowHideCompanion",
            &["Margin=-50,40,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.TextBlock#PinnedListHeaderText",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AllListHeading",
            &["Margin=0,-10,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AllListHeading > Windows.UI.Xaml.Controls.TextBlock#AllListHeadingText",
            &["Visibility=1"],
        ),
        (
            "StartMenu.CategoryControl > Windows.UI.Xaml.Controls.Grid#RootGrid > Windows.UI.Xaml.Controls.Border",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        ("Windows.UI.Xaml.Controls.Grid#MainMenu", &["Width=460"]),
        (
            "StartMenu.PinnedList#StartMenuPinnedList",
            &["Width=400", "Height=450", "Margin=0,0,0,30"],
        ),
        (
            "Windows.UI.Xaml.Controls.GridView#PinnedList > Border > Windows.UI.Xaml.Controls.ScrollViewer",
            &[
                "ScrollViewer.VerticalScrollMode=2",
                "MaxHeight:=336",
                "MinHeight:=100",
                "Width=300",
                "Margin=0,0,60,0",
            ],
        ),
        (
            "StartMenu.PinnedList#StartMenuPinnedList > Windows.UI.Xaml.Controls.Grid#Root > Windows.UI.Xaml.Controls.GridView#PinnedList",
            &["MinHeight=8400"],
        ),
        (
            "StartMenu.PinnedList#StartMenuPinnedList > Windows.UI.Xaml.Controls.Grid#Root > Windows.UI.Xaml.Controls.GridView#PinnedList > Border",
            &["VerticalAlignment=Top", "Height=450"],
        ),
        (
            "StartMenu.StartMenuCompanion#RightCompanion",
            &["Height=810", "Margin=15,0,30,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#CompanionRoot > Windows.UI.Xaml.Controls.Border#AcrylicBorder",
            &[
                "Background:=$ElementBG",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
                "CornerRadius=$ElementCornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.GridView#AllAppsGrid > Windows.UI.Xaml.Controls.ItemsWrapGrid",
            &["Visibility=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.GridView#AllAppsGrid",
            &["Margin=0,15,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#TopLevelHeader > Windows.UI.Xaml.Controls.Grid > Windows.UI.Xaml.Controls.Button",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.FlyoutPresenter",
            &[
                "BorderBrush:=$BorderBrush",
                "BorderThickness:=$BorderThickness",
                "CornerRadius:=$ElementCornerRadius",
                "Padding=-1",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.MenuFlyoutPresenter",
            &["CornerRadius:=$ElementCornerRadius"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AllListHeading > Microsoft.UI.Xaml.Controls.DropDownButton#ViewSelectionButton > Grid#RootGrid",
            &["CornerRadius=$HoverCornerRadius", "Margin=-12,0,12,0"],
        ),
        (
            "MenuFlyoutItem",
            &["CornerRadius:=$HoverCornerRadius", "Margin=4,0,4,0"],
        ),
        (
            "ToggleMenuFlyoutItem",
            &["CornerRadius:=$HoverCornerRadius", "Margin=4,0,4,0"],
        ),
    ],
    vars: VARS,
};
