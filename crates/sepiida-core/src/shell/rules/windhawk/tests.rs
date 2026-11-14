use super::*;
use crate::shell::{
    layout::Value,
    rules::{Effect, Name, Paint, Role, Step},
    tree::ElementTree,
};

const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
const BORDER: &str = "Windows.UI.Xaml.Controls.Border";

fn one(text: &'static str) -> Target {
    let mut targets = parse_targets(text).unwrap();
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
            },
            Step::AnyDepth,
            Step::Element {
                type_name: "A.Page",
                name: Name::Is("Root"),
            },
        ]
    );
}

#[test]
fn commas_split_targets_except_inside_brackets() {
    let targets =
        parse_targets("Border#ToastBackgroundBorder, Border#ToastBackgroundBorder2").unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].name, Name::Is("ToastBackgroundBorder2"));
    assert_eq!(
        parse_targets("Grid[AutomationProperties.Name=a,b]"),
        Err(SelectorError::Unsupported("[index] or [Property=Value]"))
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
            "Grid[2]",
            SelectorError::Unsupported("[index] or [Property=Value]"),
        ),
    ] {
        assert_eq!(parse_targets(text), Err(error), "{text:?}");
    }
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
    assert!(matches!(
        parse_style("Grid.Column=1"),
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
];

fn effects(target: &'static str, styles: &'static [&'static str]) -> Built {
    let theme = Theme {
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
        ("Grid#ThumbnailImage", &["Width=300", "Grid.Column=1"][..]),
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
