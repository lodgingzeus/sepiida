//! Which rule set each shell process uses, and the checks every set must
//! pass. Sets with Windhawk theme lines are built once per process, on
//! first use.

use std::sync::OnceLock;

use super::{
    flyouts::FLYOUT_OVERLAYS,
    rule::Rule,
    start::START_RULES,
    taskbar::TASKBAR_RULES,
    windhawk::{Skipped, Theme, build_all},
    window_glass::{
        FLYOUT_ADDITIONS, FLYOUT_CONTROLS, FLYOUT_PANELS, START, TASKBAR, TASKBAR_ADDITIONS,
    },
};
use crate::shell::hosts::Host;

struct RuleSet {
    rules: Vec<Rule>,
    skipped: Vec<Skipped>,
}

impl RuleSet {
    /// Our hand-written rules first, then the themes' rules.
    fn new(own: &[Rule], themes: &[&Theme]) -> Self {
        let built = build_all(themes);
        let mut rules = own.to_vec();
        rules.extend(built.rules);
        Self {
            rules,
            skipped: built.skipped,
        }
    }
}

fn taskbar() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(TASKBAR_RULES, &[&TASKBAR, &TASKBAR_ADDITIONS]))
}

fn start() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| RuleSet::new(START_RULES, &[&START]))
}

fn flyouts() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| {
        RuleSet::new(
            FLYOUT_OVERLAYS,
            &[&FLYOUT_PANELS, &FLYOUT_CONTROLS, &FLYOUT_ADDITIONS],
        )
    })
}

fn set_for(host: Host) -> &'static RuleSet {
    match host {
        Host::Explorer => taskbar(),
        Host::StartMenu => start(),
        Host::ShellHost | Host::ShellExperience => flyouts(),
    }
}

/// The rule set for elements in `host`.
pub fn rules_for(host: Host) -> &'static [Rule] {
    &set_for(host).rules
}

/// Theme entries left out of `host`'s rule set, and why; for the log.
pub fn skipped_for(host: Host) -> &'static [Skipped] {
    &set_for(host).skipped
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
        skipped_for(host)
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
        assert!(skipped(Host::Explorer).is_empty());
        let start: Vec<&str> = skipped(Host::StartMenu).iter().map(|s| s.0).collect();
        assert_eq!(
            start,
            [
                "Windows.UI.Xaml.Controls.Grid#UndockedRoot",
                "Windows.UI.Xaml.Controls.GridView#PinnedList > Border > Windows.UI.Xaml.Controls.ScrollViewer",
            ]
        );
        // Quick settings button corners ([Property=Value]), media layout
        // and the toast peek position (Grid.Row/Column), media button
        // colours (@CommonStates).
        assert_eq!(skipped(Host::ShellHost).len(), 13);
    }

    #[test]
    fn every_host_has_rules_and_something_to_paint() {
        for host in Host::ALL {
            let rules = rules_for(host);
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
            for rule in rules_for(host) {
                if let Effect::Color { paint, role } = rule.effect {
                    let card = role == Role::SurfaceRaised;
                    assert_eq!(paint == Paint::Solid, card, "{host:?}: {}", rule.label);
                }
            }
        }
    }

    #[test]
    fn no_two_rules_of_a_host_change_the_same_property_of_an_element() {
        for host in Host::ALL {
            let rules = rules_for(host);
            for (i, a) in rules.iter().enumerate() {
                for b in &rules[i + 1..] {
                    assert!(
                        !(a.type_name == b.type_name
                            && a.name == b.name
                            && a.chain == b.chain
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
        for host in Host::ALL {
            for rule in rules_for(host) {
                if let Effect::Set(value) = rule.effect {
                    assert!(value.is_valid(), "{}", rule.label);
                    looks += 1;
                }
            }
        }
        assert!(looks > 0);
    }
}
