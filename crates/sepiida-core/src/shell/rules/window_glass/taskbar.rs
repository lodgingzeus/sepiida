//! WindowGlass for the popups explorer.exe shows beside the taskbar:
//! glass tooltips, menus, hover thumbnails, the overflow flyout, Alt+Tab,
//! virtual desktops, the snap bar and the confirmation popup. The taskbar
//! itself is RosePine's (`rose_pine`); WindowGlass's lines for the tray
//! icons, app buttons and search box are gone (in git), so the two do not
//! mix. Left out from the start, because they move or size the frame or
//! the button row (during M5 step 3 that crashed Explorer once): every
//! `TaskbarFrame`, `RootGrid`, `SystemTrayFrame` and `:root` line, the
//! widgets button and the gripper; also the clock's fonts and hidden date
//! and the theme's `Button` border for every button. The glass is our own
//! acrylic, proven in Explorer.

use crate::shell::framework::Framework;
use crate::shell::rules::edge::{PANEL_EDGE, PANEL_EDGE_THICKNESS};
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    (
        "Background",
        Var::Material {
            glass: &Var::Acrylic(Role::Surface),
            solid: &Var::Fill(Role::Surface),
        },
    ),
    ("BorderBrush", PANEL_EDGE),
    ("BorderThickness", PANEL_EDGE_THICKNESS),
    ("CornerRadius", Var::Text("25")),
];

pub const TASKBAR: Theme = Theme {
    framework: Framework::System,
    material: None,
    targets: &[
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
            "Windows.UI.Xaml.Controls.ToolTip > Windows.UI.Xaml.Controls.ContentPresenter#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness:=$BorderThickness",
                "CornerRadius=12",
            ],
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
    framework: Framework::System,
    material: None,
    targets: &[(
        "Taskbar.TaskbarBackground#HoverFlyoutBackgroundControl > Grid > Rectangle#BackgroundFill",
        &["Fill:=$Background"],
    )],
    vars: VARS,
};
