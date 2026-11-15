//! Which rule set each shell process uses, per XAML framework, and the
//! checks every set must pass. Sets with Windhawk theme lines are built
//! once per process, on first use. Only explorer.exe has WinUI 3 rules
//! (File Explorer); every other host's WinUI 3 set is empty.

use std::sync::OnceLock;

use super::{
    better_control::{FLYOUTS, MATERIALS},
    down_aero::START,
    flyouts::FLYOUT_OVERLAYS,
    matter::EXPLORER as MATTER_EXPLORER,
    rose_pine::TASKBAR as ROSE_PINE_TASKBAR,
    rule::Rule,
    start::START_RULES,
    taskbar::TASKBAR_BASE,
    windhawk::{Skipped, Theme, build_on},
    window_glass::{EXPLORER, POPUPS, TASKBAR, TASKBAR_ADDITIONS},
};
use crate::shell::{framework::Framework, hosts::Host};

struct RuleSet {
    rules: Vec<Rule>,
    skipped: Vec<Skipped>,
}

impl RuleSet {
    /// Our hand-written rules first, then the themes' rules.
    fn new(own: &[Rule], themes: &[&Theme]) -> Self {
        let built = build_on(own, themes);
        Self {
            rules: built.rules,
            skipped: built.skipped,
        }
    }
}

fn taskbar() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| {
        RuleSet::new(
            TASKBAR_BASE,
            &[&TASKBAR, &TASKBAR_ADDITIONS, &ROSE_PINE_TASKBAR],
        )
    })
}

fn start() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(START_RULES, &[&START]))
}

fn flyouts() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(FLYOUT_OVERLAYS, &[&POPUPS, &FLYOUTS, &MATERIALS]))
}

fn file_explorer() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(&[], &[&EXPLORER, &MATTER_EXPLORER]))
}

fn empty() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(&[], &[]))
}

fn set_for(host: Host, framework: Framework) -> &'static RuleSet {
    match (host, framework) {
        (Host::Explorer, Framework::System) => taskbar(),
        (Host::Explorer, Framework::WinUi) => file_explorer(),
        (Host::StartMenu, Framework::System) => start(),
        (Host::ShellHost | Host::ShellExperience, Framework::System) => flyouts(),
        (_, Framework::WinUi) => empty(),
    }
}

/// The rule set for `framework`'s elements in `host`.
pub fn rules_for(host: Host, framework: Framework) -> &'static [Rule] {
    &set_for(host, framework).rules
}

/// Theme entries left out of that rule set, and why; for the log.
pub fn skipped_for(host: Host, framework: Framework) -> &'static [Skipped] {
    &set_for(host, framework).skipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::{Effect, Paint, Role};

    /// Colour and look rules may share a property: a look value is put
    /// over the coloured one while the look is on (a theme's blur).
    fn is_look(rule: &Rule) -> bool {
        matches!(rule.effect, Effect::Set(_))
    }

    fn paint(rule: &Rule) -> Option<Paint> {
        match rule.effect {
            Effect::Color { paint, .. } => Some(paint),
            Effect::Set(_) | Effect::Probe | Effect::Measure(_) => None,
        }
    }

    fn skipped(host: Host) -> Vec<(&'static str, &'static str)> {
        skipped_in(host, Framework::System)
    }

    fn skipped_in(host: Host, framework: Framework) -> Vec<(&'static str, &'static str)> {
        skipped_for(host, framework)
            .iter()
            .map(|s| {
                let feature = KNOWN_GAPS
                    .iter()
                    .copied()
                    .find(|gap| s.reason.contains(gap))
                    .unwrap_or("unexpected");
                (s.target, feature)
            })
            .collect()
    }

    /// Engine features the pasted themes use and we lack; the last is an
    /// empty value (`Background=`), which Windhawk reads as "clear".
    const KNOWN_GAPS: [&str; 4] = [
        "attached property",
        "[Property=Value]",
        "@VisualState",
        "\"\" is not a plain value",
    ];

    /// Theme entries left out are known: each is a feature the engine
    /// lacks. A new skip means a pasted line needs a look.
    #[test]
    fn only_known_theme_entries_are_left_out() {
        for host in Host::ALL {
            for (target, feature) in skipped(host) {
                assert_ne!(feature, "unexpected", "{host:?}: {target}");
            }
        }
        // RosePine's running-app ring (`rose_pine`).
        assert_eq!(
            skipped(Host::Explorer),
            [(
                "Grid#IconPanel@CommonStates > Rectangle#RunningIndicator, Taskbar.TaskListLabeledButtonPanel@CommonStates > Rectangle#RunningIndicator",
                "@VisualState",
            )]
        );
        assert!(skipped(Host::StartMenu).is_empty());
        assert!(skipped(Host::ShellHost).is_empty());
    }

    #[test]
    fn every_host_has_rules_and_something_to_paint() {
        for host in Host::ALL {
            let rules = rules_for(host, Framework::System);
            assert!(
                rules
                    .iter()
                    .any(|r| paint(r).is_some_and(|p| p != Paint::LogOnly)),
                "{host:?}"
            );
        }
    }

    /// Surfaces stay see-through: only cards are painted solid, and cards
    /// are never tinted.
    #[test]
    fn surfaces_are_tinted_and_only_cards_are_solid() {
        for host in Host::ALL {
            for rule in rules_for(host, Framework::System) {
                if let Effect::Color { paint, role } = rule.effect {
                    let card = role == Role::SurfaceRaised;
                    assert_eq!(paint == Paint::Solid, card, "{host:?}: {}", rule.label);
                }
            }
        }
    }

    /// Every rule set that has rules: each host's system XAML set, and
    /// File Explorer's.
    fn every_set() -> Vec<(Host, Framework)> {
        let mut sets: Vec<_> = Host::ALL.map(|h| (h, Framework::System)).to_vec();
        sets.push((Host::Explorer, Framework::WinUi));
        sets
    }

    #[test]
    fn file_explorer_has_winui_rules_and_no_other_host_does() {
        let rules = rules_for(Host::Explorer, Framework::WinUi);
        assert!(rules.len() > 50, "{}", rules.len());
        // Bare type names became WinUI 3 names; the one system XAML type
        // is written out in the theme (it never matches in Explorer).
        for rule in rules {
            let winui = rule.type_name.starts_with("Microsoft.UI.Xaml.")
                || rule.type_name.starts_with("FileExplorerExtensions.");
            assert!(
                winui || rule.label.contains("RootCommandSearchGrid"),
                "{}",
                rule.label
            );
        }
        for host in [Host::StartMenu, Host::ShellHost, Host::ShellExperience] {
            assert!(rules_for(host, Framework::WinUi).is_empty(), "{host:?}");
        }
        let left_out: Vec<_> = skipped_in(Host::Explorer, Framework::WinUi)
            .into_iter()
            .map(|(_, feature)| feature)
            .collect();
        // Two of WindowGlass's, two of Matter's (tab and search box states).
        assert_eq!(left_out, ["@VisualState"; 4]);
    }

    /// Whether two rules' materials meet (both hold at some time).
    fn overlap(
        a: Option<crate::shell::rules::Material>,
        b: Option<crate::shell::rules::Material>,
    ) -> bool {
        a.is_none() || b.is_none() || a == b
    }

    /// Colours hold whatever the material; only look rules are for one.
    #[test]
    fn colour_rules_hold_for_both_materials_and_both_have_looks() {
        use crate::shell::rules::Material;
        for (host, framework) in every_set() {
            let rules = rules_for(host, framework);
            for rule in rules {
                if let Effect::Color { .. } = rule.effect {
                    assert_eq!(rule.material, None, "{host:?}: {}", rule.label);
                }
            }
            for material in [Material::Glass, Material::Solid] {
                assert!(
                    rules.iter().any(|r| r.material == Some(material)),
                    "{host:?} {framework:?}: no {material:?} rules"
                );
            }
        }
    }

    /// Every theme XAML value is one the DLL will load.
    #[test]
    fn every_xaml_value_is_a_document() {
        use crate::shell::layout::Value;
        for (host, framework) in every_set() {
            for rule in rules_for(host, framework) {
                if let Effect::Set(Value::Xaml(text)) = rule.effect {
                    assert!(
                        crate::shell::markup::document(text).is_some(),
                        "{host:?}: {}: {text}",
                        rule.label
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_rules_of_a_host_change_the_same_property_of_an_element() {
        for (host, framework) in every_set() {
            let rules = rules_for(host, framework);
            for (i, a) in rules.iter().enumerate() {
                for b in &rules[i + 1..] {
                    assert!(
                        !(overlap(a.material, b.material)
                            && a.type_name == b.type_name
                            && a.name == b.name
                            && a.chain == b.chain
                            && a.position == b.position
                            && a.conditions == b.conditions
                            && a.property == b.property
                            && is_look(a) == is_look(b)),
                        "{host:?}: {} and {}",
                        a.label,
                        b.label
                    );
                }
            }
        }
    }

    #[test]
    fn every_look_value_is_valid() {
        let mut looks = 0;
        for (host, framework) in every_set() {
            for rule in rules_for(host, framework) {
                if let Effect::Set(value) = rule.effect {
                    assert!(value.is_valid(), "{}", rule.label);
                    looks += 1;
                }
            }
        }
        assert!(looks > 0);
    }
}
