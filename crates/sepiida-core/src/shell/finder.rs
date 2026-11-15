//! Finds the elements a rule set styles in the stream of visual tree
//! changes XAML Diagnostics reports.
//!
//! Elements can arrive in any order: a candidate (right type and name) may
//! be reported before the ancestor its rule requires. Such candidates are
//! kept as pending and re-tested after every later addition, but only while
//! an ancestor is still missing: a candidate whose chain reaches the root
//! without the ancestor never matches (unnamed elements are common, and
//! would otherwise crowd out real candidates).

use std::collections::HashSet;

use super::{
    rules::{Rule, RuleId, Step},
    tree::{ElementTree, Handle, Verdict},
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
    /// The element's child index, when a rule asks for a position of an
    /// element of its type (`Type[n]`), for the log. Only the first few.
    pub positioned: Option<u32>,
}

/// Child indices reported in the log, to check `Type[n]` on Windows.
const MAX_POSITION_REPORTS: usize = 40;

/// Every type named with a position, as the element or in a chain.
fn positioned_types(rules: &[Rule]) -> Vec<&'static str> {
    let mut types = Vec::new();
    for rule in rules {
        if rule.position.is_some() {
            types.push(rule.type_name);
        }
        for step in rule.chain {
            if let Step::Element {
                type_name,
                position: Some(_),
                ..
            } = *step
            {
                types.push(type_name);
            }
        }
    }
    types.sort_unstable();
    types.dedup();
    types
}

#[derive(Debug)]
pub struct Finder {
    rules: &'static [Rule],
    tree: ElementTree,
    pending: Vec<Match>,
    matched: HashSet<Match>,
    anchors_seen: HashSet<&'static str>,
    near_misses: usize,
    /// Candidates not kept for a re-test because `pending` was full.
    pending_dropped: usize,
    /// Types some rule asks a position of, and how many were reported.
    positioned_types: Vec<&'static str>,
    positioned_reports: usize,
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
            pending_dropped: 0,
            positioned_types: positioned_types(rules),
            positioned_reports: 0,
        }
    }

    pub fn rule(&self, id: RuleId) -> Option<&'static Rule> {
        self.rules.get(id)
    }

    /// The element's place in the tree, for the log.
    pub fn path(&self, handle: Handle) -> String {
        self.tree.path(handle)
    }

    /// The element's parent, if it has one and it is known.
    pub fn parent(&self, handle: Handle) -> Option<Handle> {
        self.tree.parent(handle)
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

    /// Candidates dropped because too many were pending; any of them may
    /// have been a target that is now never styled.
    pub fn pending_dropped_count(&self) -> usize {
        self.pending_dropped
    }

    pub fn added(&mut self, handle: Handle, parent: Handle, type_name: &str, name: &str) -> Added {
        self.added_at(handle, parent, None, type_name, name)
    }

    /// `added`, with the element's place among its parent's children
    /// (for `Type[n]` selectors).
    pub fn added_at(
        &mut self,
        handle: Handle,
        parent: Handle,
        index: Option<u32>,
        type_name: &str,
        name: &str,
    ) -> Added {
        let mut result = Added::default();
        if !self.tree.add_at(handle, parent, index, type_name, name) {
            return result;
        }
        result.first_anchor = self.note_anchor(name);
        if self.positioned_reports < MAX_POSITION_REPORTS
            && self.positioned_types.contains(&type_name)
        {
            self.positioned_reports += 1;
            result.positioned = index;
        }
        for (rule, spec) in self.rules.iter().enumerate() {
            let candidate = Match { handle, rule };
            if !spec.selects(type_name, name)
                || self.matched.contains(&candidate)
                || self.pending.contains(&candidate)
            {
                continue;
            }
            match self.tree.verdict(handle, spec) {
                Verdict::Yes => {
                    self.matched.insert(candidate);
                    result.matched.push(candidate);
                }
                verdict => {
                    self.near_misses += 1;
                    if self.near_misses <= MAX_NEAR_MISS_REPORTS {
                        result.near_miss = Some(self.tree.path(handle));
                    }
                    if verdict == Verdict::No {
                        continue;
                    }
                    if self.pending.len() < MAX_PENDING {
                        self.pending.push(candidate);
                    } else {
                        self.pending_dropped += 1;
                    }
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
        let verdict = |m: &Match| {
            rules
                .get(m.rule)
                .map_or(Verdict::No, |r| tree.verdict(m.handle, r))
        };
        let now_matched: Vec<Match> = self
            .pending
            .iter()
            .copied()
            .filter(|m| verdict(m) == Verdict::Yes)
            .collect();
        // Those ruled out by the ancestors now known never match.
        self.pending.retain(|m| verdict(m) == Verdict::NotYet);
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
mod tests;
