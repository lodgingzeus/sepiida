//! WindowGlass for the flyouts, second half: calendar days, list items,
//! sliders, switches and buttons. Left out: the `AccessKey` lines on
//! `Button#ClearAll` and `Button#ExpandCollapseButton` (new shortcuts are
//! a feature, not a look).

use super::flyout_panels::FLYOUT_VARS;
use crate::shell::framework::Framework;
use crate::shell::rules::windhawk::Theme;

pub const FLYOUT_CONTROLS: Theme = Theme {
    framework: Framework::System,
    targets: &[
        (
            "Windows.UI.Xaml.Controls.CalendarViewDayItem > Windows.UI.Xaml.Controls.Border",
            &["CornerRadius=8", "Margin=1,2,1,2"],
        ),
        (
            "Windows.UI.Xaml.Controls.CalendarViewDayItem",
            &["CornerRadius=8"],
        ),
        (
            "Windows.UI.Xaml.Controls.Control > Windows.UI.Xaml.Controls.Border",
            &["CornerRadius=8"],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.CalendarViewItem",
            &["CornerRadius=8"],
        ),
        (
            "Windows.UI.Xaml.Controls.ListViewHeaderItem",
            &["Margin=50,6,50,2", "CornerRadius=8", "Height=35"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#SettingsButton",
            &["CornerRadius=4"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#DismissButton",
            &["CornerRadius=4"],
        ),
        (
            "Windows.UI.Xaml.Controls.StackPanel#CalendarHeader",
            &["Margin=6,0,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.ScrollContentPresenter#ScrollContentPresenter",
            &["Margin=1,2,1,2"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#WeekDayNames",
            &[
                "Background:=$ElementSysColor",
                "CornerRadius=8",
                "Margin=4,0,4,0",
                "Padding=0,-5,0,-3",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.ListViewItem",
            &["CornerRadius=$CR3"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#RootGrid > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter",
            &[
                "Background:=<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight1}\" Opacity=\"0.5\"/>",
                "BorderThickness=0",
                "CornerRadius=8",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid > Windows.UI.Xaml.Controls.Border#ItemOpaquePlating",
            &[
                "Background:=$overlay2",
                "BorderThickness=0",
                "CornerRadius=$CR3",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#StandardHeroContainer",
            &["Margin=12,0,12,0", "CornerRadius=0", "Height=150"],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.ScrollBar#VerticalScrollBar",
            &["Visibility=1"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#SliderContainer",
            &["Margin=0-2,0,0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#BackButton",
            &["CornerRadius=$CR3"],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#OuterBorder",
            &["RadiusX=8", "RadiusY=8", "Height=18"],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#SwitchKnobOff",
            &["RadiusY=8", "RadiusX=8"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#SwitchKnobOn",
            &["CornerRadius=8"],
        ),
        (
            "Windows.UI.Xaml.Shapes.Rectangle#SwitchKnobBounds",
            &["RadiusX=8", "RadiusY=8", "Height=18"],
        ),
        ("ActionCenter.NotificationListViewItem", &["Margin=5,2,5,3"]),
        (
            "Windows.UI.Xaml.Controls.Grid[AutomationProperties.LocalizedLandmarkType=Footer]",
            &["BorderThickness=0"],
        ),
        (
            "NetworkUX.View.SettingsListViewItem > Windows.UI.Xaml.Controls.Primitives.ListViewItemPresenter#Root",
            &["CornerRadius=12"],
        ),
        (
            "Windows.UI.Xaml.Controls.ContentPresenter > Windows.UI.Xaml.Controls.Border",
            &["BorderThickness=0"],
        ),
        (
            "ControlCenter.PaginatedToggleButton#ToggleButton > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter",
            &[
                "CornerRadius=$CR2",
                "BorderThickness=$ElementBorderThickness",
                "BorderBrush:=$ElementBorderBrush",
            ],
        ),
        (
            "ControlCenter.PaginatedToggleButton#SplitL2Button > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter",
            &[
                "CornerRadius=30",
                "BorderThickness=$ElementBorderThickness",
                "BorderBrush:=$ElementBorderBrush",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.Thumb#HorizontalThumb > Windows.UI.Xaml.Controls.Border",
            &[
                "CornerRadius=12",
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.Thumb#HorizontalThumb > Windows.UI.Xaml.Controls.Border > Windows.UI.Xaml.Shapes.Ellipse#SliderInnerThumb",
            &["Visibility=Collapsed"],
        ),
        (
            "Windows.UI.Xaml.Controls.ToolTip > Windows.UI.Xaml.Controls.ContentPresenter#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=10",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.RepeatButton#PreviousButton > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter@CommonStates",
            &[
                "Foreground@Normal:=$ElementSysColor",
                "Foreground@PointerOver:=$ElementSysColor2",
                "Foreground@Pressed:=$ElementSysColor3",
                "Foreground@Disabled:=$ElementSysColor4",
                "Background=Transparent",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Button#PlayPauseButton > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter@CommonStates",
            &[
                "Foreground@Normal:=$ElementSysColor",
                "Foreground@PointerOver:=$ElementSysColor2",
                "Foreground@Pressed:=$ElementSysColor3",
                "Foreground@Disabled:=$ElementSysColor4",
                "Background=Transparent",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Primitives.RepeatButton#NextButton > Windows.UI.Xaml.Controls.ContentPresenter#ContentPresenter@CommonStates",
            &[
                "Foreground@Normal:=$ElementSysColor",
                "Foreground@PointerOver:=$ElementSysColor2",
                "Foreground@Pressed:=$ElementSysColor3",
                "Foreground@Disabled:=$ElementSysColor4",
                "Background=Transparent",
            ],
        ),
        ("Grid#ControlCenterRegion", &["Grid.Row=0"]),
        (
            "ControlCenter.MediaTransportControls",
            &["VerticalAlignment=2", "Grid.Row=1", "Canvas.ZIndex=1"],
        ),
        ("Grid#RootGrid", &["VerticalAlignment=3", "MinHeight=0"]),
    ],
    vars: FLYOUT_VARS,
};
