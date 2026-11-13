//! Finds the elements a rule set styles in the stream of visual tree
//! changes XAML Diagnostics reports.
//!
//! Elements can arrive in any order: a candidate (right type and name) may
//! be reported before the ancestor its rule requires. Such candidates are
//! kept as pending and re-tested after every later addition.

use std::collections::HashSet;

use super::{
    rules::{Rule, RuleId},
    tree::{ElementTree, Handle},
};

/// Unmatched candidates re-tested on every addition. One per taskbar hover
/// flyout is normal; the cap only bounds the per-callback cost.
pub const MAX_PENDING: usize = 64;
/// Near misses reported for the log. Explorer re-creates the hover
/// flyout's rectangle on every tree change, which flooded the log in M3.
pub const MAX_NEAR_MISS_REPORTS: usize = 3;

/// An element to style, and the rule it matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Match {
    pub handle: Handle,
    pub rule: RuleId,
}

/// What one addition revealed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Added {
    /// Elements that became known as targets with this addition.
    pub matched: Vec<Match>,
    /// The first element of this tree carrying a name some rule requires
    /// of an ancestor (e.g. the first `TaskbarFrame`).
    pub first_anchor: Option<&'static str>,
    /// A candidate that is not (yet) under its required ancestor, with its
    /// path, for the log. Only the first few are reported.
    pub near_miss: Option<String>,
}

#[derive(Debug)]
pub struct Finder {
    rules: &'static [Rule],
    tree: ElementTree,
    pending: Vec<Match>,
    matched: HashSet<Match>,
    anchors_seen: HashSet<&'static str>,
    near_misses: usize,
}

impl Finder {
    pub fn new(rules: &'static [Rule]) -> Self {
        Self {
            rules,
            tree: ElementTree::new(),
            pending: Vec::new(),
            matched: HashSet::new(),
            anchors_seen: HashSet::new(),
            near_misses: 0,
        }
    }

    pub fn rule(&self, id: RuleId) -> Option<&'static Rule> {
        self.rules.get(id)
    }

    pub fn element_count(&self) -> usize {
        self.tree.len()
    }

    /// Whether any element a rule requires as an ancestor has been seen.
    pub fn saw_anchor(&self) -> bool {
        !self.anchors_seen.is_empty()
    }

    pub fn matched_count(&self) -> usize {
        self.matched.len()
    }

    /// Candidates seen outside their required ancestor, reported or not.
    pub fn near_miss_count(&self) -> usize {
        self.near_misses
    }

    pub fn added(&mut self, handle: Handle, parent: Handle, type_name: &str, name: &str) -> Added {
        let mut result = Added::default();
        if !self.tree.add(handle, parent, type_name, name) {
            return result;
        }
        result.first_anchor = self.note_anchor(name);
        for (rule, spec) in self.rules.iter().enumerate() {
            let candidate = Match { handle, rule };
            if !spec.selects(type_name, name)
                || self.matched.contains(&candidate)
                || self.pending.contains(&candidate)
            {
                continue;
            }
            if self.tree.matches(handle, spec) {
                self.matched.insert(candidate);
                result.matched.push(candidate);
            } else {
                self.near_misses += 1;
                if self.near_misses <= MAX_NEAR_MISS_REPORTS {
                    result.near_miss = Some(self.tree.path(handle));
                }
                if self.pending.len() < MAX_PENDING {
                    self.pending.push(candidate);
                }
            }
        }
        self.retest_pending(&mut result);
        result
    }

    fn note_anchor(&mut self, name: &str) -> Option<&'static str> {
        let anchor = self
            .rules
            .iter()
            .filter_map(|rule| rule.ancestor)
            .find(|&anchor| anchor == name)?;
        self.anchors_seen.insert(anchor).then_some(anchor)
    }

    fn retest_pending(&mut self, result: &mut Added) {
        if self.pending.is_empty() {
            return;
        }
        let (tree, rules) = (&self.tree, self.rules);
        let (now_matched, still_pending): (Vec<Match>, Vec<Match>) = self
            .pending
            .iter()
            .partition(|m| rules.get(m.rule).is_some_and(|r| tree.matches(m.handle, r)));
        self.pending = still_pending;
        for found in now_matched {
            if self.matched.insert(found) {
                result.matched.push(found);
            }
        }
    }

    /// Forgets an element and its subtree. Returns the targets that went
    /// with it.
    pub fn removed(&mut self, handle: Handle) -> Vec<Match> {
        let removed = self.tree.remove(handle);
        if removed.is_empty() {
            return Vec::new();
        }
        let removed: HashSet<Handle> = removed.into_iter().collect();
        self.pending.retain(|m| !removed.contains(&m.handle));
        let gone: Vec<Match> = self
            .matched
            .iter()
            .copied()
            .filter(|m| removed.contains(&m.handle))
            .collect();
        for m in &gone {
            self.matched.remove(m);
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::{
        RECTANGLE_TYPE, Role, TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME, TASKBAR_RULES,
    };

    const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
    const FRAME_TYPE: &str = "Taskbar.TaskbarFrame";
    const BACKGROUND_TYPE: &str = "Taskbar.TaskbarBackground";
    const FILL: &str = TASKBAR_BACKGROUND.name;

    fn taskbar_finder() -> Finder {
        Finder::new(TASKBAR_RULES)
    }

    fn hit(handle: Handle) -> Match {
        Match { handle, rule: 0 }
    }

    #[test]
    fn parents_first_matches_on_the_rectangle() {
        let mut finder = taskbar_finder();
        assert_eq!(
            finder
                .added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME)
                .first_anchor,
            Some(TASKBAR_FRAME_NAME)
        );
        finder.added(2, 1, GRID, "RootGrid");
        finder.added(3, 2, BACKGROUND_TYPE, "BackgroundControl");
        finder.added(4, 3, GRID, "");
        let added = finder.added(5, 4, RECTANGLE_TYPE, FILL);
        assert_eq!(added.matched, vec![hit(5)]);
        assert_eq!(added.near_miss, None);
        assert!(finder.saw_anchor());
        assert_eq!(finder.matched_count(), 1);
        assert_eq!(finder.rule(0), Some(&TASKBAR_BACKGROUND));
    }

    #[test]
    fn an_intermediate_ancestor_arriving_last_still_matches() {
        let mut finder = taskbar_finder();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        let early = finder.added(5, 4, RECTANGLE_TYPE, FILL);
        assert!(early.matched.is_empty());
        assert!(early.near_miss.is_some());
        finder.added(4, 3, GRID, "");
        // Grid 3 links 4 to the frame; nothing is named specially.
        let late = finder.added(3, 1, BACKGROUND_TYPE, "BackgroundControl");
        assert_eq!(late.matched, vec![hit(5)]);
    }

    #[test]
    fn a_match_and_an_anchor_are_reported_once() {
        let mut finder = taskbar_finder();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        assert_eq!(
            finder.added(5, 1, RECTANGLE_TYPE, FILL).matched,
            vec![hit(5)]
        );
        assert!(finder.added(5, 1, RECTANGLE_TYPE, FILL).matched.is_empty());
        assert!(finder.added(6, 1, GRID, "").matched.is_empty());
        assert_eq!(
            finder
                .added(7, 0, FRAME_TYPE, TASKBAR_FRAME_NAME)
                .first_anchor,
            None
        );
    }

    #[test]
    fn the_hover_flyout_is_a_near_miss_that_never_matches() {
        let mut finder = taskbar_finder();
        finder.added(10, 0, BACKGROUND_TYPE, "HoverFlyoutBackgroundControl");
        let added = finder.added(11, 10, RECTANGLE_TYPE, FILL);
        assert!(added.matched.is_empty());
        assert_eq!(
            added.near_miss.as_deref(),
            Some(
                "Windows.UI.Xaml.Shapes.Rectangle#BackgroundFill \
                 < Taskbar.TaskbarBackground#HoverFlyoutBackgroundControl"
            )
        );
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        assert_eq!(finder.matched_count(), 0);
    }

    #[test]
    fn only_the_first_near_misses_are_reported() {
        let mut finder = taskbar_finder();
        let reported = (1..=10)
            .filter(|&n| {
                let flyout = 100 + n;
                finder.added(flyout, 0, BACKGROUND_TYPE, "HoverFlyout");
                finder
                    .added(n, flyout, RECTANGLE_TYPE, FILL)
                    .near_miss
                    .is_some()
            })
            .count();
        assert_eq!(reported, MAX_NEAR_MISS_REPORTS);
        assert_eq!(finder.near_miss_count(), 10);
    }

    #[test]
    fn removal_reports_the_targets_that_went() {
        let mut finder = taskbar_finder();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        finder.added(2, 1, GRID, "");
        finder.added(5, 2, RECTANGLE_TYPE, FILL);
        finder.added(9, 0, GRID, "");
        assert!(finder.removed(9).is_empty());
        assert_eq!(finder.removed(1), vec![hit(5)]);
        assert_eq!(finder.matched_count(), 0);
        assert!(finder.removed(1).is_empty());
    }

    #[test]
    fn removed_pending_candidates_are_not_rechecked() {
        let mut finder = taskbar_finder();
        finder.added(5, 4, RECTANGLE_TYPE, FILL);
        finder.removed(5);
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        // Handle 5 reused by an unrelated element under the frame.
        assert!(finder.added(5, 1, GRID, "").matched.is_empty());
        assert_eq!(finder.matched_count(), 0);
    }

    #[test]
    fn pending_is_capped() {
        let mut finder = taskbar_finder();
        for handle in 1..=(MAX_PENDING as Handle + 10) {
            finder.added(handle, 1000, RECTANGLE_TYPE, FILL);
        }
        assert_eq!(finder.pending.len(), MAX_PENDING);
    }

    static TWO_RULES: [Rule; 2] = [
        TASKBAR_BACKGROUND,
        Rule {
            label: "anywhere",
            type_name: "Windows.UI.Xaml.Controls.Border",
            name: "Acrylic",
            ancestor: None,
            property: "Background",
            role: Role::Accent,
        },
    ];

    #[test]
    fn a_rule_without_an_ancestor_matches_at_once_and_ids_follow_the_rule_set() {
        let mut finder = Finder::new(&TWO_RULES);
        let added = finder.added(8, 0, "Windows.UI.Xaml.Controls.Border", "Acrylic");
        assert_eq!(added.matched, vec![Match { handle: 8, rule: 1 }]);
        assert_eq!(finder.rule(1).map(|r| r.role), Some(Role::Accent));
        assert_eq!(finder.rule(2), None);
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        assert_eq!(
            finder.added(5, 1, RECTANGLE_TYPE, FILL).matched,
            vec![hit(5)]
        );
    }
}
