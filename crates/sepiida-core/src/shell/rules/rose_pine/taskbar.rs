//! RosePine for Explorer's taskbar.

use crate::shell::framework::Framework;
use crate::shell::rules::{
    Role,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    (
        "Pill",
        Var::Material {
            glass: &Var::Acrylic(Role::Surface),
            solid: &Var::Fill(Role::Surface),
        },
    ),
    (
        "Ring",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemAccentColorLight2}\" />"),
    ),
    (
        "LabelText",
        Var::Xaml("<SolidColorBrush Color=\"{ThemeResource SystemBaseHighColor}\" />"),
    ),
];

pub const TASKBAR: Theme = Theme {
    framework: Framework::System,
    material: None,
    targets: &[
        ("Taskbar.TaskListButton", &["CornerRadius=3"]),
        (
            "SystemTray.TextIconContent > Grid#ContainerGrid > SystemTray.AdaptiveTextBlock#Base > TextBlock#InnerTextBlock",
            &["FontSize=16"],
        ),
        ("SystemTray.NotifyIconView#NotifyItemIcon", &["MinWidth=25"]),
        (
            "SystemTray.OmniButton#ControlCenterButton > Grid > ContentPresenter > ItemsPresenter > StackPanel > ContentPresenter[1] > SystemTray.IconView > Grid > Grid",
            &["Visibility=Collapsed"],
        ),
        (
            "SystemTray.TextIconContent > Grid#ContainerGrid",
            &["Padding=2"],
        ),
        ("SystemTray.ChevronIconView", &["MinWidth=27"]),
        (
            "SystemTray.OmniButton#NotificationCenterButton > Grid > ContentPresenter > ItemsPresenter > StackPanel > ContentPresenter > SystemTray.IconView#SystemTrayIcon > Grid > Grid > SystemTray.TextIconContent",
            &["Visibility=Collapsed"],
        ),
        (
            "Grid#IconPanel > Border#BackgroundElement, Taskbar.TaskListLabeledButtonPanel > Border#BackgroundElement",
            &["Background:=$Pill", "CornerRadius=6"],
        ),
        (
            "StackPanel#SystemTrayFrameGrid, Grid#SystemTrayFrameGrid",
            &[
                "Background:=$Pill",
                "CornerRadius=6",
                "Margin=0,5,4,4",
                "Padding=3,0,-8,0",
            ],
        ),
        (
            "Grid#IconPanel@CommonStates > Rectangle#RunningIndicator, Taskbar.TaskListLabeledButtonPanel@CommonStates > Rectangle#RunningIndicator",
            &[
                "Height=Auto",
                "RadiusX=5",
                "RadiusY=5",
                "StrokeThickness=2",
                "Stroke@InactivePointerOver:=$Ring",
                "Stroke@InactivePressed:=$Ring",
                "Stroke@ActiveNormal:=$Ring",
                "Stroke@ActivePointerOver:=$Ring",
                "Stroke@ActivePressed:=$Ring",
                "Fill=Transparent",
                "Width=37",
                "VerticalAlignment=Stretch",
                "Canvas.ZIndex=1",
            ],
        ),
        (
            "SystemTray.ImageIconContent > Grid#ContainerGrid > Image",
            &["Width=13"],
        ),
        (
            "SystemTray.TextIconContent > Grid#ContainerGrid > SystemTray.AdaptiveTextBlock#Base > TextBlock#InnerTextBlock",
            &["FontSize=14"],
        ),
        (
            "TextBlock#LabelControl",
            &["Foreground:=$LabelText", "Padding=2,0,8,0"],
        ),
        (
            "Taskbar.ExperienceToggleButton#LaunchListButton[AutomationProperties.AutomationId=StartButton]",
            &["Visibility=Collapsed"],
        ),
        (
            "TextBlock#InnerTextBlock[Text=\u{E971}], TextBlock#InnerTextBlock[Text=\u{E972}]",
            &["Text=\u{E712}"],
        ),
        (
            "Taskbar.TaskbarFrame > Grid#RootGrid > Taskbar.TaskbarBackground > Grid > Rectangle#BackgroundFill",
            &["Fill=Transparent"],
        ),
        ("Rectangle#BackgroundStroke", &["Fill=Transparent"]),
        (
            "Taskbar.TaskListButtonPanel#ExperienceToggleButtonRootPanel > Grid > Border#BackgroundElement, Taskbar.TaskListButtonPanel#ExperienceToggleButtonRootPanel > Border#BackgroundElement, SearchUx.SearchUI.SearchButtonRootGrid#SearchBoxButtonRootPanel > Grid > Border#BackgroundElement, SearchUx.SearchUI.SearchButtonRootGrid#SearchBoxButtonRootPanel > Border#BackgroundElement",
            &["Background:=$Pill"],
        ),
        ("TextBlock#DateInnerTextBlock", &["Margin=0,0,0,-2"]),
        ("Grid#OverflowRootGrid > Border", &["Background:=$Pill"]),
        // Ours, as RosePine's screenshot shows the bar: no search box and
        // no widgets button. Lines from the same mod's TaskbarToStatusbar
        // theme, but the search button's `[AutomationId=SearchButton]`
        // dropped: a condition on an ancestor is not supported, and the
        // type and name already pick the one button.
        (
            "Taskbar.SearchBoxButton#SearchBoxButton > Taskbar.TaskListButtonPanel",
            &["Visibility=Collapsed"],
        ),
        (
            "Taskbar.AugmentedEntryPointButton#AugmentedEntryPointButton",
            &["Visibility=Collapsed"],
        ),
    ],
    vars: VARS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::{Name, Step, windhawk::build};

    #[test]
    fn only_the_running_indicator_waits_for_visual_states() {
        let built = build(&TASKBAR);
        assert_eq!(built.skipped.len(), 1);
        assert!(built.skipped[0].reason.contains("@VisualState"));
    }

    /// RosePine's removals are in: the Start button, the network icon,
    /// the notification-centre count and the tray glyph swap.
    #[test]
    fn the_theme_hides_what_rose_pine_hides() {
        let built = build(&TASKBAR);
        let hidden = |part: &str| {
            built.rules.iter().any(|r| {
                r.property == "Visibility"
                    && (r.type_name.contains(part)
                        || r.conditions.iter().any(|c| c.value == part)
                        || r.chain.iter().any(
                            |s| matches!(s, Step::Element { name: Name::Is(n), .. } if *n == part),
                        ))
            })
        };
        assert!(hidden("StartButton"));
        assert!(hidden("ControlCenterButton"));
        assert!(hidden("NotificationCenterButton"));
        assert!(hidden("SearchBoxButton"));
        assert!(hidden("AugmentedEntryPointButton"));
        let glyphs: Vec<_> = built
            .rules
            .iter()
            .filter(|r| r.property == "Text" && !r.conditions.is_empty())
            .collect();
        assert_eq!(glyphs.len(), 2);
    }

    /// No colour or font of the theme remains.
    #[test]
    fn no_colour_or_font_of_the_theme_remains() {
        for (target, styles) in TASKBAR.targets {
            for style in *styles {
                assert!(!style.contains('#'), "{target}: {style}");
                assert!(!style.starts_with("FontFamily"), "{target}: {style}");
            }
        }
    }
}
