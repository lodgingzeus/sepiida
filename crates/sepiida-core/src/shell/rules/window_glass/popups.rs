//! WindowGlass for the popups the shell hosts beside quick settings and
//! the notification centre: toasts, jump lists, context menus and
//! tooltips, as rounded glass. Quick settings, the notification centre,
//! the calendar and the media card take BetterControl11's layout
//! (`better_control`); WindowGlass's lines for them are gone (in git),
//! and so is its toast peek position (a layout move, not a look).

use crate::shell::framework::Framework;
use crate::shell::rules::edge::{PANEL_EDGE, PANEL_EDGE_THICKNESS};
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
        Var::Material {
            glass: &Var::Blur {
                amount: 5.0,
                role: Role::Surface,
                opacity: 0.7,
            },
            solid: &Var::Fill(Role::Surface),
        },
    ),
    ("BorderBrush", PANEL_EDGE),
    ("BorderThickness", PANEL_EDGE_THICKNESS),
    ("CR3", Var::Text("12")),
];

pub const POPUPS: Theme = Theme {
    framework: Framework::System,
    material: None,
    targets: &[
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
        (
            "Windows.UI.Xaml.Controls.ToolTip > Windows.UI.Xaml.Controls.ContentPresenter#LayoutRoot",
            &[
                "Background:=$Background",
                "BorderBrush:=$BorderBrush",
                "BorderThickness=$BorderThickness",
                "CornerRadius=10",
            ],
        ),
    ],
    vars: VARS,
};
