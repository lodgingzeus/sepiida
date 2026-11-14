//! Which rule set each shell process uses, and the checks every set must
//! pass. Sets with Windhawk theme lines are built once per process, on
//! first use.

use std::sync::OnceLock;

use super::{
    flyouts::{ADDITIONS, FLYOUT_OVERLAYS, TINTED_GLASS},
    rule::Rule,
    start::START_RULES,
    taskbar::TASKBAR_RULES,
    windhawk::{Skipped, build},
};
use crate::shell::hosts::Host;

struct RuleSet {
    rules: Vec<Rule>,
    skipped: Vec<Skipped>,
}

fn flyouts() -> &'static RuleSet {
    static SET: OnceLock<RuleSet> = OnceLock::new();
    SET.get_or_init(|| {
        let (mut rules, mut skipped) = (Vec::new(), Vec::new());
        for theme in [&TINTED_GLASS, &ADDITIONS] {
            let built = build(theme);
            rules.extend(built.rules);
            skipped.extend(built.skipped);
        }
        rules.extend_from_slice(FLYOUT_OVERLAYS);
        RuleSet { rules, skipped }
    })
}

/// The rule set for elements in `host`.
pub fn rules_for(host: Host) -> &'static [Rule] {
    match host {
        Host::Explorer => TASKBAR_RULES,
        Host::StartMenu => START_RULES,
        Host::ShellHost | Host::ShellExperience => &flyouts().rules,
    }
}

/// Theme entries left out of `host`'s rule set, and why; for the log.
pub fn skipped_for(host: Host) -> &'static [Skipped] {
    match host {
        Host::Explorer | Host::StartMenu => &[],
        Host::ShellHost | Host::ShellExperience => &flyouts().skipped,
    }
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

    /// Theme entries left out are known: each is a feature the engine
    /// lacks. A new skip means a pasted line needs a look.
    #[test]
    fn only_known_theme_entries_are_left_out() {
        let flyouts: Vec<&str> = skipped_for(Host::ShellHost)
            .iter()
            .map(|s| s.target)
            .collect();
        // Both set `Grid.Column` (attached properties): media thumbnail
        // layout.
        assert_eq!(
            flyouts,
            [
                "Grid#ThumbnailImage",
                "StackPanel#PrimaryAndSecondaryTextContainer"
            ]
        );
        assert!(skipped_for(Host::Explorer).is_empty());
        assert!(skipped_for(Host::StartMenu).is_empty());
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
