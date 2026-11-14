//! WindowGlass's taskbar details, over our own floating bar and tray pill
//! (`rules::taskbar`): tray icon padding and corners, rounded app buttons,
//! a slimmer search box, and glass tooltips, menus, hover thumbnails,
//! Alt+Tab, virtual desktops and the snap bar. The padding and the search
//! box change the size of their own buttons only; the bar's end is
//! measured from the tray's width, so it follows. Left out, because they
//! move or size the frame, the button row or the tray panel (plan.md, M5
//! step 3: that crashed Explorer once): every `TaskbarFrame`, `RootGrid`,
//! `SystemTrayFrame`, tray panel and `:root` line, the widgets button and
//! the gripper; also the clock's fonts and hidden date, the search box
//! text, and the theme's `Button` border for every button. The glass is
//! our own acrylic, proven in Explorer, not the blur brush (not yet run on
//! Windows).

use super::edge::{PANEL_EDGE, PANEL_EDGE_THICKNESS};
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    ("Background", Var::Acrylic(Role::Surface)),
    ("BorderBrush", PANEL_EDGE),
    (
        "ElementBorderBrush",
        Var::Xaml(
            "<LinearGradientBrush StartPoint=\"0,0\" EndPoint=\"0,1\"><GradientStop Color=\"#50808080\" Offset=\"1\" /><GradientStop Color=\"#50606060\" Offset=\"0.15\" /></LinearGradientBrush>",
        ),
    ),
    ("ElementBorderThickness", Var::Text("0.3,0.3,0.3,1")),
    ("ElementCornerRadius", Var::Text("12")),
    ("BorderThickness", PANEL_EDGE_THICKNESS),
    ("CornerRadius", Var::Text("25")),
    ("TrayPadding", Var::Text("2,4,2,4")),
];

pub const TASKBAR: Theme = Theme {
    targets: &[
        (
            "SystemTray.ChevronIconView",
            &["Padding=6,0,6,0", "CornerRadius=8", "Margin=2,0,0,0"],
        ),
        (
            "SystemTray.NotifyIconView#NotifyItemIcon",
            &["Padding=6,0,6,0", "CornerRadius=8", "Margin=2,0,0,0"],
        ),
        (
            "SystemTray.OmniButton",
            &["Padding=6,0,6,0", "CornerRadius=8", "Margin=2,0,0,0"],
        ),
        (
            "SystemTray.CopilotIcon",
            &["Padding=6,0,6,0", "Margin=2,0,0,0"],
        ),
        (
            "SystemTray.OmniButton#NotificationCenterButton > Grid > ContentPresenter > ItemsPresenter > StackPanel > ContentPresenter > SystemTray.IconView#SystemTrayIcon > Grid",
            &["Padding=$TrayPadding"],
        ),
        (
            "SystemTray.IconView#SystemTrayIcon > Grid#ContainerGrid > ContentPresenter#ContentPresenter > Grid#ContentGrid > SystemTray.TextIconContent > Grid#ContainerGrid",
            &["Padding=10", "CornerRadius=10"],
        ),
        (
            "SystemTray.StackListView#IconStack > ItemsPresenter > StackPanel > ContentPresenter > SystemTray.IconView#SystemTrayIcon",
            &["Padding=0"],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#ConfirmatorMainGrid",
            &[
                "CornerRadius=22",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "Background:=$Background",
            ],
        ),
        (
            "WindowsInternal.ComposableShell.Experiences.Switcher.AltTab > Windows.UI.Xaml.Controls.Grid#ModalRootGrid > Windows.UI.Xaml.Controls.Border#BackgroundElement",
            &[
                "BorderThickness=0",
                "Background=Transparent",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "WindowsInternal.ComposableShell.Experiences.Switcher.AltTab > Windows.UI.Xaml.Controls.Grid#ModalRootGrid > Windows.UI.Xaml.Controls.Border#BackgroundElement > WindowsInternal.ComposableShell.Experiences.Switcher.SwitchItemList",
            &[
                "Background:=$Background",
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "WindowsInternal.ComposableShell.Experiences.Switcher.VirtualDesktopBarElement#VirtualDesktopBar > Grid > Border",
            &[
                "BorderThickness=$BorderThickness",
                "BorderBrush:=$BorderBrush",
                "Background:=$Background",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#BackgroundDimmingLayer",
            &["Background:=Transparent"],
        ),
        (
            "Taskbar.TaskListButton#TaskListButton",
            &["CornerRadius=10"],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#SnapBarBorder",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
                "RenderTransform:=<TranslateTransform X=\"0\" Y=\"10\" />",
                "Margin=0,0,0,-10",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#SnapPickerBorder",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "CornerRadius=$CornerRadius",
                "BorderThickness=$BorderThickness",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#SearchPillBackgroundElement",
            &[
                "BorderBrush:=$ElementBorderBrush",
                "CornerRadius=$ElementCornerRadius",
                "BorderThickness=$ElementBorderThickness",
                "MaxWidth:=100",
                "Width=Auto",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.ToolTip > Windows.UI.Xaml.Controls.ContentPresenter#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness:=$BorderThickness",
                "CornerRadius=12",
            ],
        ),
        (
            "SearchUx.SearchUI.SearchButtonControl",
            &["MaxWidth:=300", "MinWidth:=10", "Width=Auto", "Margin=0"],
        ),
        (
            "WindowsInternal.ComposableShell.Experiences.Switcher.VirtualDesktopBarElement > Windows.UI.Xaml.Controls.Grid#GridElement > Windows.UI.Xaml.Controls.Border#VirtualDesktopSwitcherBackground",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Border#OverflowFlyoutBackgroundBorder",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.MenuFlyoutPresenter > Windows.UI.Xaml.Controls.Border",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
        (
            "Windows.UI.Xaml.Controls.Grid#HoverFlyoutGrid > Windows.UI.Xaml.Controls.Border#HoverFlyoutBackground",
            &[
                "Background:=Transparent",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=$CornerRadius",
            ],
        ),
    ],
    vars: VARS,
};

/// Ours: WindowGlass clears the hover thumbnails' border and paints their
/// glass on the flyout's fill rectangle, with a selector that would also
/// take the taskbar's own fill; this one takes only the flyout's.
pub const TASKBAR_ADDITIONS: Theme = Theme {
    targets: &[(
        "Taskbar.TaskbarBackground#HoverFlyoutBackgroundControl > Grid > Rectangle#BackgroundFill",
        &["Fill:=$Background"],
    )],
    vars: VARS,
};
