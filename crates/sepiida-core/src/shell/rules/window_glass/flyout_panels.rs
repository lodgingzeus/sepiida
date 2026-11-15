//! WindowGlass for the flyouts, first half: the panels (notification
//! centre, calendar, quick settings, media, toasts, jump lists) as
//! separate rounded glass cards. `FLYOUT_CONTROLS` continues the theme.
//! Left out: `FontFamily` lines (vivo Sans).

use super::edge::{PANEL_EDGE, PANEL_EDGE_THICKNESS};
use crate::shell::framework::Framework;
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

pub(super) const FLYOUT_VARS: &[(&str, Var)] = &[
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
        "overlay",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeAltHighColor}\" Opacity=\"0.1\" />",
        ),
    ),
    // Ours: notification cards stay opaque in the raised surface colour
    // (the theme blurs them), as before WindowGlass.
    ("overlay2", Var::Solid(Role::SurfaceRaised)),
    // Ours, for `FLYOUT_ADDITIONS`' sliders.
    (
        "SliderTrack",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemBaseHighColor}\" Opacity=\"0.25\" />",
        ),
    ),
    (
        "SliderThumb",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemChromeWhiteColor}\" />"),
    ),
    ("CornerRadius", Var::Text("20")),
    ("CR2", Var::Text("14")),
    ("CR3", Var::Text("12")),
    ("BorderThickness", PANEL_EDGE_THICKNESS),
    (
        "ElementBG",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemChromeAltHighColor}\" Opacity=\"0.3\" />",
        ),
    ),
    (
        "ElementBorderBrush",
        Var::Xaml(
            "<LinearGradientBrush StartPoint=\"0,0\" EndPoint=\"0,1\"><GradientStop Color=\"#50808080\" Offset=\"1\" /><GradientStop Color=\"#50606060\" Offset=\"0.15\" /></LinearGradientBrush>",
        ),
    ),
    ("ElementCornerRadius", Var::Text("20")),
    ("ElementBorderThickness", Var::Text("0.3,0.3,0.3,1")),
    (
        "ElementSysColor",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" Opacity=\"1\" />",
        ),
    ),
    (
        "ElementSysColor2",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight2}\" Opacity=\"1\" />",
        ),
    ),
    (
        "ElementSysColor3",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight3}\" Opacity=\"1\" />",
        ),
    ),
    (
        "ElementSysColor4",
        Var::Xaml(
            "<SolidColorBrush Color=\"{ThemeResource SystemAccentColorDark1}\" Opacity=\"1\" />",
        ),
    ),
];

pub const FLYOUT_PANELS: Theme = Theme {
    framework: Framework::System,
    targets: &[
        (
            "Grid#NotificationCenterGrid",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "Grid#CalendarCenterGrid",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
                "Margin=0,6,0,6",
                "MinHeight=40",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "ScrollViewer#CalendarControlScrollViewer",
            &[
                "Background:=$ElementBG",
                "CornerRadius=$CR2",
                "Margin=-10,11,-10,-14",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
            ],
        ),
        (
            "Border#CalendarHeaderMinimizedOverlay",
            &[
                "Background:=$ElementBG",
                "CornerRadius=$CR2",
                "Margin=-10,-6,-10,-8",
                "Height=45",
                "BorderBrush:=$ElementBorderBrush",
                "BorderThickness=$ElementBorderThickness",
            ],
        ),
        (
            "ActionCenter.FocusSessionControl#FocusSessionControl > Grid#FocusGrid",
            &[
                "Background:=$Background",
                "CornerRadius=$CR2",
                "Margin=6,7,6,6",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "MenuFlyoutPresenter > Border",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CR3",
                "Padding=1,2,1,2",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        ("MenuFlyoutItem > Grid#LayoutRoot", &["CornerRadius=6"]),
        (
            "Border#JumpListRestyledAcrylic",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CR3",
                "Margin=-2,-2,-2,-2",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#ControlCenterRegion",
            &[
                "Background:=$Background",
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        ("ContentPresenter#PageContent", &["Background=Transparent"]),
        (
            "ContentPresenter#PageContent > Grid > Border",
            &[
                "Background:=$overlay",
                "CornerRadius=$CR2",
                "Margin=8,0,8,2",
            ],
        ),
        (
            "QuickActions.ControlCenter.AccessibleWindow#PageWindow > ContentPresenter > Grid#FullScreenPageRoot",
            &["Background=Transparent"],
        ),
        (
            "QuickActions.ControlCenter.AccessibleWindow#PageWindow > ContentPresenter > Grid#FullScreenPageRoot > ContentPresenter#PageHeader",
            &[
                "Background:=$overlay",
                "CornerRadius=$CR2",
                "Margin=7,7,7,7",
            ],
        ),
        (
            "ScrollViewer#ListContent",
            &[
                "Background:=$overlay",
                "CornerRadius=$CR2",
                "Margin=8,0,8,0",
            ],
        ),
        (
            "ActionCenter.FlexibleToastView#FlexibleNormalToastView",
            &["Background=Transparent"],
        ),
        (
            "Border#ToastBackgroundBorder, Border#ToastBackgroundBorder2",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "CornerRadius=16",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "JumpViewUI.SystemItemListViewItem > Grid#LayoutRoot > Border#BackgroundBorder",
            &["CornerRadius=8"],
        ),
        (
            "JumpViewUI.JumpListListViewItem > Grid#LayoutRoot > Border#BackgroundBorder",
            &["CornerRadius=6"],
        ),
        ("ActionCenter.FlexibleItemView", &["CornerRadius=16"]),
        (
            "Grid#NotificationCenterTopBanner",
            &["Background=Transparent", "CornerRadius=$CR2", "Margin=6"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#L1Grid > Border",
            &["Background=Transparent"],
        ),
        (
            "Windows.UI.Xaml.Controls.ContentPresenter",
            &["BorderThickness=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#FooterButton[AutomationProperties.Name=Edit quick settings]",
            &["Margin=0,0,8,0", "CornerRadius=$CR3"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button[AutomationProperties.AutomationId=Microsoft.QuickAction.Battery]",
            &["Margin=2,0,0,0", "CornerRadius=$CR3"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#FooterButton[AutomationProperties.Name=All settings]",
            &[
                "Margin=0,0,-1,0",
                "CornerRadius=13",
                "BorderThickness=$BorderThickness",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Button[AutomationProperties.AutomationId=Microsoft.QuickAction.Volume]",
            &["CornerRadius=10"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#VolumeL2Button[AutomationProperties.Name=Select a sound output]",
            &["CornerRadius=10"],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#HorizontalTrackRect",
            &[
                "Height=10",
                "Fill:=$overlay",
                "RadiusY=5",
                "RadiusX=5",
                "Margin=0,-10,10,-10",
            ],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#HorizontalDecreaseRect",
            &[
                "Height=10",
                "RadiusY=5",
                "RadiusX=5",
                "Margin=0,-10,-10,-10",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.Thumb#HorizontalThumb",
            &["Visibility=Visible", "Height=25", "Width=40", "Margin=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#MediaTransportControlsRegion",
            &[
                "Height=100",
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "Background:=$Background",
                "Margin=0,10,0,0",
                "BorderBrush:=$BorderBrush",
                "Grid.Row=1",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#AlbumTextAndArtContainer",
            &["Height=55", "MaxWidth=150", "HorizontalAlignment=Left"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#ThumbnailImage",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.StackPanel#PrimaryAndSecondaryTextContainer",
            &[
                "VerticalAlignment=Center",
                "HorizontalAlignment=Left",
                "Margin=0,0,10,0",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.StackPanel#PrimaryAndSecondaryTextContainer > Windows.UI.Xaml.Controls.TextBlock#Title",
            &["TextAlignment=Center", "FontSize=18"],
        ),
        (
            "Windows.UI.Xaml.Controls.StackPanel#PrimaryAndSecondaryTextContainer > Windows.UI.Xaml.Controls.TextBlock#Subtitle",
            &["TextAlignment=Center", "Margin=0,3,0,0", "FontWeight=600"],
        ),
        (
            "Windows.UI.Xaml.Controls.ListView#MediaButtonsListView",
            &[
                "VerticalAlignment=Center",
                "Height=20",
                "Margin=130,-60,0,0",
                "Width=Auto",
                "HorizontalAlignment=Right",
                "Visibility=2",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.RepeatButton#PreviousButton",
            &["Width=40", "Height=40", "Margin=10,0,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#PlayPauseButton",
            &["Width=40", "Height=40", "Margin=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.RepeatButton#NextButton",
            &["Width=40", "Height=30", "Margin=0,0,10,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.TextBlock#AppNameText",
            &["FontSize=16"],
        ),
        (
            "Windows.UI.Xaml.Controls.Image#IconImage",
            &["Height=20", "Width=20"],
        ),
        (
            "Grid#MediaTransportControlsRoot",
            &["Background=Transparent"],
        ),
        (
            "Grid#ToastPeekRegion",
            &[
                "Background=",
                "RenderTransform:=<TranslateTransform Y=\"-495\" X=\"395\" />",
                "Grid.Column=0",
                "Grid.Row=2",
            ],
        ),
    ],
    vars: FLYOUT_VARS,
};

/// Ours, after the theme (a later line wins):
/// - the media card: the theme's entry also sets `Grid.Row` (an attached
///   property we cannot set yet), which skips it whole; this keeps its
///   glass, border and corners (not its height and row);
/// - sliders: the theme's track is a 10 % veil, invisible on the glass,
///   and its thumb is the blurred glass itself, a smudge; here a 25 %
///   track in the base colour and a white pill.
pub const FLYOUT_ADDITIONS: Theme = Theme {
    framework: Framework::System,
    targets: &[
        (
            "Windows.UI.Xaml.Controls.Grid#MediaTransportControlsRegion",
            &[
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
            ],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#HorizontalTrackRect",
            &["Fill:=$SliderTrack"],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.Thumb#HorizontalThumb > Windows.UI.Xaml.Controls.Border",
            &["Background:=$SliderThumb"],
        ),
    ],
    vars: FLYOUT_VARS,
};
