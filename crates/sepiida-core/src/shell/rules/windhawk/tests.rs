use super::*;
use crate::shell::{
    framework::Framework,
    layout::Value,
    rules::{Condition, Effect, Material, Name, Paint, Role, Step},
    tree::ElementTree,
};

const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
const BORDER: &str = "Windows.UI.Xaml.Controls.Border";

fn one(text: &'static str) -> Target {
    let mut targets = parse_targets(text, Framework::System).unwrap();
    assert_eq!(targets.len(), 1, "{text}");
    targets.remove(0)
}

#[test]
fn a_bare_type_is_a_xaml_control_and_names_are_optional() {
    let target = one("Grid#L1Grid");
    assert_eq!(target.type_name, GRID);
    assert_eq!(target.name, Name::Is("L1Grid"));
    assert!(target.chain.is_empty());
    assert_eq!(one("Border").name, Name::Any);
    assert_eq!(
        one("Rectangle").type_name,
        "Windows.UI.Xaml.Shapes.Rectangle"
    );
    assert_eq!(
        one("ActionCenter.FlexibleItemView").type_name,
        "ActionCenter.FlexibleItemView"
    );
    assert_eq!(
        one("muxc:PipsPager#QuickActionsPager").type_name,
        "Microsoft.UI.Xaml.Controls.PipsPager"
    );
}

#[test]
fn ancestors_are_listed_nearest_first() {
    let target = one("A.Page#Root > * > Grid#L1Grid > Border");
    assert_eq!(target.type_name, BORDER);
    assert_eq!(
        target.chain,
        &[
            Step::Element {
                type_name: GRID,
                name: Name::Is("L1Grid"),
                position: None,
            },
            Step::AnyDepth,
            Step::Element {
                type_name: "A.Page",
                name: Name::Is("Root"),
                position: None,
            },
        ]
    );
}

#[test]
fn commas_split_targets_except_inside_brackets() {
    let targets = parse_targets(
        "Border#ToastBackgroundBorder, Border#ToastBackgroundBorder2",
        Framework::System,
    )
    .unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].name, Name::Is("ToastBackgroundBorder2"));
    let targets = parse_targets(
        "Grid[AutomationProperties.Name=a,b], Border",
        Framework::System,
    )
    .unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(
        targets[0].conditions,
        &[Condition {
            property: "AutomationProperties.Name",
            value: "a,b",
        }]
    );
}

#[test]
fn property_conditions_go_on_the_element() {
    let targets = parse_targets(
        "Taskbar.ExperienceToggleButton#LaunchListButton[AutomationProperties.AutomationId=StartButton]",
        Framework::System,
    )
    .unwrap();
    assert_eq!(targets[0].name, Name::Is("LaunchListButton"));
    assert_eq!(
        targets[0].conditions,
        &[Condition {
            property: "AutomationProperties.AutomationId",
            value: "StartButton",
        }]
    );
    // A value may hold the markers; two conditions and an index mix.
    let targets = parse_targets("TextBlock[Text=#a@b][2][Tag = x ]", Framework::System).unwrap();
    assert_eq!(targets[0].position, Some(1));
    assert_eq!(
        targets[0].conditions,
        &[
            Condition {
                property: "Text",
                value: "#a@b",
            },
            Condition {
                property: "Tag",
                value: "x",
            },
        ]
    );
    assert_eq!(
        parse_targets("TextBlock[Text=\u{E971}]", Framework::System).unwrap()[0].conditions[0]
            .value,
        "\u{E971}"
    );
}

#[test]
fn bad_and_unsupported_selectors_are_reported() {
    for (text, error) in [
        ("", SelectorError::Empty),
        ("#Name", SelectorError::EmptyType),
        ("Grid#", SelectorError::EmptyName),
        ("Grid#a#b", SelectorError::TwoNames),
        ("Grid > *", SelectorError::MisplacedWildcard),
        ("* > Grid", SelectorError::MisplacedWildcard),
        ("Grid > * > * > Grid", SelectorError::MisplacedWildcard),
        (":root > Grid", SelectorError::Unsupported(":root")),
        (
            "Button@CommonStates",
            SelectorError::Unsupported("@VisualState"),
        ),
        (
            "Grid[Text=a] > Border",
            SelectorError::Unsupported("[Property=Value] on an ancestor"),
        ),
        ("Grid[=a]", SelectorError::BadCondition),
        ("Grid[Text=]", SelectorError::BadCondition),
        ("Grid[Text=a", SelectorError::BadIndex),
        ("Grid[0]", SelectorError::BadIndex),
        ("Grid[x]", SelectorError::BadIndex),
        ("Grid[-1]", SelectorError::BadIndex),
        ("Grid[2", SelectorError::BadIndex),
        ("Grid[1][2]", SelectorError::BadIndex),
    ] {
        assert_eq!(
            parse_targets(text, Framework::System),
            Err(error),
            "{text:?}"
        );
    }
}

#[test]
fn a_child_index_is_kept_zero_based_on_the_element_and_its_ancestors() {
    let target = one("Grid > GridViewItem[2] > Border[ 1 ]#Inner");
    assert_eq!(target.type_name, BORDER);
    assert_eq!(target.name, Name::Is("Inner"));
    assert_eq!(target.position, Some(0));
    assert_eq!(
        target.chain[0],
        Step::Element {
            type_name: "Windows.UI.Xaml.Controls.GridViewItem",
            name: Name::Any,
            position: Some(1),
        }
    );
    assert_eq!(one("Grid").position, None);
}

#[test]
fn styles_split_into_property_and_value() {
    assert_eq!(
        parse_style("CornerRadius=14"),
        Ok(Style {
            property: "CornerRadius",
            value: StyleValue::Text("14"),
        })
    );
    assert_eq!(
        parse_style("Background:=$Base"),
        Ok(Style {
            property: "Background",
            value: StyleValue::Xaml("$Base"),
        })
    );
    assert_eq!(parse_style("Background"), Err(StyleError::NoEquals));
    assert_eq!(parse_style("=1"), Err(StyleError::EmptyProperty));
    assert_eq!(
        parse_style("Grid.Column = 1"),
        Ok(Style {
            property: "Grid.Column",
            value: StyleValue::Text("1"),
        })
    );
    assert!(matches!(
        parse_style("A.B.C=1"),
        Err(StyleError::Unsupported(_))
    ));
    assert!(matches!(
        parse_style("Fill@PointerOver=Red"),
        Err(StyleError::Unsupported(_))
    ));
    assert!(matches!(
        parse_style("Width=>w"),
        Err(StyleError::Unsupported(_))
    ));
}

const VARS: &[(&str, Var)] = &[
    ("Base", Var::Tint(Role::Surface)),
    ("Card", Var::Solid(Role::SurfaceRaised)),
    ("Transparent", Var::Transparent),
    ("Radius", Var::Text("14")),
    (
        "Glass",
        Var::Blur {
            amount: 18.0,
            role: Role::Surface,
            opacity: 0.5,
        },
    ),
    ("Pane", Var::Acrylic(Role::Surface)),
    ("Edge", Var::Xaml("<SolidColorBrush Color=\"#50808080\" />")),
    (
        "Bad",
        Var::Xaml("<ImageBrush ImageSource=\"https://x/a.png\"/>"),
    ),
];

fn effects(target: &'static str, styles: &'static [&'static str]) -> Built {
    let theme = Theme {
        framework: Framework::System,
        material: None,
        targets: Box::leak(vec![(target, styles)].into_boxed_slice()),
        vars: VARS,
    };
    build(&theme)
}

#[test]
fn variables_become_our_colours_and_text_becomes_look_values() {
    let built = effects(
        "Grid#NotificationCenterGrid",
        &[
            "Background:=$Base",
            "CornerRadius=$Radius",
            "BorderThickness=0,0,0,0",
            "Height=Auto",
        ],
    );
    assert!(built.skipped.is_empty(), "{:?}", built.skipped);
    let got: Vec<(&str, Effect)> = built.rules.iter().map(|r| (r.property, r.effect)).collect();
    assert_eq!(
        got,
        [
            (
                "Background",
                Effect::Color {
                    paint: Paint::Tint,
                    role: Role::Surface
                }
            ),
            ("CornerRadius", Effect::Set(Value::Text("14"))),
            ("BorderThickness", Effect::Set(Value::Text("0,0,0,0"))),
            ("Height", Effect::Set(Value::Auto)),
        ]
    );
    let clear = effects("Grid#L1Grid > Border", &["Background:=$Transparent"]);
    assert_eq!(clear.rules[0].effect, Effect::Set(Value::Transparent));
    let literal = effects("Border", &["Background=Transparent"]);
    assert_eq!(literal.rules[0].effect, Effect::Set(Value::Transparent));
    let markup = effects(
        "Border",
        &["Background:=<SolidColorBrush Color=\"Transparent\" />"],
    );
    assert_eq!(markup.rules[0].effect, Effect::Set(Value::Transparent));
    let xaml = effects("Border", &["Background:=<AcrylicBrush TintColor=\"Red\"/>"]);
    assert_eq!(
        xaml.rules[0].effect,
        Effect::Set(Value::Xaml("<AcrylicBrush TintColor=\"Red\"/>"))
    );
}

#[test]
fn an_entry_with_anything_unsupported_is_skipped_whole() {
    for (target, styles) in [
        (
            "Grid#ThumbnailImage",
            &["Width=300", "Grid.Column:=<Grid/>"][..],
        ),
        (
            "Grid",
            &["Background:=<WindhawkBlur BlurAmount=\"9999\"/>"][..],
        ),
        (
            "Grid",
            &["Background:=<ImageBrush ImageSource=\"https://x/a.png\"/>"][..],
        ),
        ("Grid", &["Background:=$Unknown"][..]),
        ("Grid", &["Shadow:="][..]),
        ("Grid", &["Tag={ThemeResource X}"][..]),
        ("Grid@CommonStates", &["Width=1"][..]),
    ] {
        let styles: &'static [&'static str] = Box::leak(styles.to_vec().into_boxed_slice());
        let built = effects(target, styles);
        assert!(built.rules.is_empty(), "{target} {styles:?}");
        assert_eq!(built.skipped.len(), 1, "{target} {styles:?}");
        assert_eq!(built.skipped[0].target, target);
    }
}

/// The quick settings tiles area as dumped on build 26200, plus a Border
/// that is not under `L1Grid`.
fn quick_settings() -> ElementTree {
    let mut tree = ElementTree::new();
    tree.add(1, 0, "ControlCenter.ControlCenterView", "ControlCenterView");
    tree.add(2, 1, GRID, "RootGrid");
    tree.add(3, 2, GRID, "L1Grid");
    tree.add(4, 3, BORDER, "");
    tree.add(5, 3, GRID, "Tiles");
    tree.add(6, 5, BORDER, "Deep");
    tree.add(7, 2, BORDER, "");
    tree
}

#[test]
fn a_direct_parent_chain_matches_only_children() {
    let tree = quick_settings();
    let rules = effects("Grid#L1Grid > Border", &["Background:=$Transparent"]).rules;
    let mut matched = tree.matching(&rules[0]);
    matched.sort_unstable();
    assert_eq!(matched, [4]);
}

#[test]
fn a_wildcard_matches_any_depth_including_none() {
    let tree = quick_settings();
    let rules = effects(
        "ControlCenter.ControlCenterView > * > Border",
        &["Background:=$Transparent"],
    )
    .rules;
    let mut matched = tree.matching(&rules[0]);
    matched.sort_unstable();
    // 7 sits right under RootGrid (one level skipped), 4 and 6 deeper.
    assert_eq!(matched, [4, 6, 7]);
    let rules = effects("Grid#RootGrid > * > Border#Deep", &["Width=1"]).rules;
    assert_eq!(tree.matching(&rules[0]), [6]);
}

#[test]
fn a_cycle_in_the_reported_parents_does_not_hang() {
    let mut tree = ElementTree::new();
    tree.add(1, 2, GRID, "A");
    tree.add(2, 1, GRID, "B");
    tree.add(3, 1, BORDER, "");
    let rules = effects("Grid#Missing > * > Border", &["Width=1"]).rules;
    assert!(tree.matching(&rules[0]).is_empty());
}

#[test]
fn a_blur_variable_tints_always_and_blurs_with_the_look() {
    let built = effects("Grid#NotificationCenterGrid", &["Background:=$Glass"]);
    let got: Vec<Effect> = built.rules.iter().map(|r| r.effect).collect();
    assert_eq!(
        got,
        [
            Effect::Color {
                paint: Paint::Tint,
                role: Role::Surface
            },
            Effect::Set(Value::Blur {
                amount: 18.0,
                role: Role::Surface,
                opacity: 0.5
            }),
        ]
    );
}

#[test]
fn a_windhawk_blur_keeps_its_amount_and_tint_alpha() {
    let built = effects(
        "Border",
        &["Background:=<WindhawkBlur BlurAmount=\"30\" TintColor=\"#33000000\"/>"],
    );
    assert_eq!(
        built.rules[0].effect,
        Effect::Set(Value::Blur {
            amount: 30.0,
            role: Role::Surface,
            opacity: 0.2
        })
    );
}

#[test]
fn a_plain_value_after_colon_equals_is_read_like_equals() {
    let built = effects(
        "Border",
        &[
            "CornerRadius:=7",
            "Background:=Transparent",
            "MaxWidth:=Auto",
        ],
    );
    assert!(built.skipped.is_empty(), "{:?}", built.skipped);
    let got: Vec<Effect> = built.rules.iter().map(|r| r.effect).collect();
    assert_eq!(
        got,
        [
            Effect::Set(Value::Text("7")),
            Effect::Set(Value::Transparent),
            Effect::Set(Value::Auto),
        ]
    );
}

#[test]
fn text_variables_are_replaced_inside_a_value() {
    let built = effects("Grid", &["CornerRadius=$Radius,0,0,$Radius"]);
    assert_eq!(built.rules[0].effect, Effect::Set(Value::Text("14,0,0,14")));
    for styles in [&["Margin=$Radius,$Nope"][..], &["Margin=$Radius,$Base"][..]] {
        let styles: &'static [&'static str] = Box::leak(styles.to_vec().into_boxed_slice());
        let built = effects("Grid", styles);
        assert!(built.rules.is_empty(), "{styles:?}");
        assert_eq!(built.skipped.len(), 1, "{styles:?}");
    }
}

#[test]
fn acrylic_and_xaml_variables_are_look_values() {
    let built = effects("Border", &["Background:=$Pane", "BorderBrush:=$Edge"]);
    let got: Vec<Effect> = built.rules.iter().map(|r| r.effect).collect();
    assert_eq!(
        got,
        [
            Effect::Set(Value::Acrylic(Role::Surface)),
            Effect::Set(Value::Xaml("<SolidColorBrush Color=\"#50808080\" />")),
        ]
    );
    let refused = effects("Border", &["Background:=$Bad"]);
    assert!(refused.rules.is_empty());
    assert_eq!(refused.skipped.len(), 1);
}

#[test]
fn a_later_line_for_the_same_target_and_property_wins() {
    let theme = Theme {
        framework: Framework::System,
        material: None,
        targets: &[
            ("Grid#A", &["Width=1", "Height=2"]),
            ("Grid#B", &["Width=3"]),
            ("Grid#A", &["Width=4", "Background:=$Glass"]),
            ("Grid#A", &["Background:=$Base"]),
        ],
        vars: VARS,
    };
    let built = build(&theme);
    let got: Vec<(&str, &str, Effect)> = built
        .rules
        .iter()
        .map(|r| (r.label, r.property, r.effect))
        .collect();
    let blur = Value::Blur {
        amount: 18.0,
        role: Role::Surface,
        opacity: 0.5,
    };
    let tint = Effect::Color {
        paint: Paint::Tint,
        role: Role::Surface,
    };
    assert_eq!(
        got,
        [
            ("Grid#A", "Height", Effect::Set(Value::Text("2"))),
            ("Grid#B", "Width", Effect::Set(Value::Text("3"))),
            ("Grid#A", "Width", Effect::Set(Value::Text("4"))),
            // The blur's look value stays; its tint is replaced by the
            // later colour of the same kind.
            ("Grid#A", "Background", Effect::Set(blur)),
            ("Grid#A", "Background", tint),
        ]
    );
}

#[test]
fn lines_for_other_children_or_conditions_do_not_replace_each_other() {
    let theme = Theme {
        framework: Framework::System,
        material: None,
        targets: &[
            ("Grid[1], Grid[Tag=a]", &["Width=1"]),
            ("Grid[2], Grid[Tag=b]", &["Width=2"]),
            ("Grid[2]", &["Width=3"]),
        ],
        vars: VARS,
    };
    let got: Vec<(&str, Effect)> = build(&theme)
        .rules
        .iter()
        .map(|r| (r.label, r.effect))
        .collect();
    assert_eq!(
        got,
        [
            ("Grid[1], Grid[Tag=a]", Effect::Set(Value::Text("1"))),
            ("Grid[1], Grid[Tag=a]", Effect::Set(Value::Text("1"))),
            ("Grid[2], Grid[Tag=b]", Effect::Set(Value::Text("2"))),
            ("Grid[2]", Effect::Set(Value::Text("3"))),
        ]
    );
}

#[test]
fn a_theme_line_replaces_a_hand_written_rule_for_the_same_slot() {
    use crate::shell::rules::sandboxed::tinted;
    let theme = Theme {
        framework: Framework::System,
        material: None,
        targets: &[("Border#A", &["Background:=$Base"])],
        vars: VARS,
    };
    let base = [tinted("Mine A", BORDER, "A"), tinted("Mine B", BORDER, "B")];
    let built = build_on(&base, &[&theme]);
    let labels: Vec<&str> = built.rules.iter().map(|r| r.label).collect();
    assert_eq!(labels, ["Mine B", "Border#A"]);
}

#[test]
fn an_attached_property_is_a_plain_look_value_in_system_xaml_only() {
    let built = effects(
        "ContentControl#SlidersGroup",
        &["Grid.Row=0", "Canvas.ZIndex:=-99"],
    );
    let got: Vec<(&str, Effect)> = built.rules.iter().map(|r| (r.property, r.effect)).collect();
    assert_eq!(
        got,
        [
            ("Grid.Row", Effect::Set(Value::Text("0"))),
            ("Canvas.ZIndex", Effect::Set(Value::Text("-99"))),
        ]
    );
    let winui = Theme {
        framework: Framework::WinUi,
        material: None,
        targets: &[("Grid#A", &["Grid.Row=0"])],
        vars: VARS,
    };
    let built = build(&winui);
    assert!(built.rules.is_empty());
    assert!(
        built.skipped[0].reason.contains("WinUI 3"),
        "{}",
        built.skipped[0].reason
    );
}

#[test]
fn colours_on_brush_properties_become_brushes() {
    let built = effects(
        "Border",
        &[
            "Background:=#09FFFFFF",
            "BorderBrush=transparent",
            "Fill=Red",
            "Width=Red",
        ],
    );
    let got: Vec<(&str, Effect)> = built.rules.iter().map(|r| (r.property, r.effect)).collect();
    assert_eq!(
        got,
        [
            (
                "Background",
                Effect::Set(Value::Xaml("<SolidColorBrush Color=\"#09FFFFFF\" />"))
            ),
            ("BorderBrush", Effect::Set(Value::Transparent)),
            (
                "Fill",
                Effect::Set(Value::Xaml("<SolidColorBrush Color=\"Red\" />"))
            ),
            ("Width", Effect::Set(Value::Text("Red"))),
        ]
    );
}

#[test]
fn a_later_line_replaces_both_twins_or_narrows_a_rule_for_both() {
    const PAIR: Var = Var::Material {
        glass: &Var::Acrylic(Role::Surface),
        solid: &Var::Fill(Role::Surface),
    };
    let vars: &'static [(&str, Var)] = &[("Pair", PAIR), ("Glass", Var::Acrylic(Role::Accent))];
    let theme = Theme {
        framework: Framework::System,
        material: None,
        targets: &[
            ("Border#A", &["Background:=$Pair"]),
            ("Border#A", &["Background=Red"]),
            ("Border#B", &["Background=Red"]),
        ],
        vars,
    };
    let later = Theme {
        framework: Framework::System,
        material: Some(Material::Glass),
        targets: &[("Border#B", &["Background:=$Glass"])],
        vars,
    };
    let built = build_all(&[&theme, &later]);
    let got: Vec<(&str, Option<Material>)> =
        built.rules.iter().map(|r| (r.label, r.material)).collect();
    assert_eq!(
        got,
        [
            // The untagged later line replaced both of A's twins.
            ("Border#A", None),
            // B's line for both now holds for solid only; glass is the
            // later theme's.
            ("Border#B", Some(Material::Solid)),
            ("Border#B", Some(Material::Glass)),
        ]
    );
}
