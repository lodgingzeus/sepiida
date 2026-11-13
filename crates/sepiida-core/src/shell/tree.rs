//! The XAML visual tree of a shell process, as XAML Diagnostics reports it.
//!
//! The hook DLL receives one callback per element added to or removed from
//! the tree, each naming the element, its parent, its type and its `x:Name`.
//! This module keeps that information so the DLL can tell which elements to
//! style. Everything here is plain data, so the matching rules are tested on
//! Linux.

use std::collections::HashMap;

use super::rules::Rule;

/// XAML Diagnostics' `InstanceHandle`. Zero means "no element".
pub type Handle = u64;

/// A shell process holds a few thousand elements. This cap only stops a
/// broken or hostile stream of callbacks from growing memory inside the
/// shell process without bound.
pub const MAX_ELEMENTS: usize = 200_000;
/// Ancestors `ElementTree::path` lists before stopping.
pub const MAX_PATH_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub parent: Option<Handle>,
    pub type_name: Box<str>,
    pub name: Box<str>,
}

#[derive(Debug, Default)]
pub struct ElementTree {
    elements: HashMap<Handle, Element>,
}

impl ElementTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn get(&self, handle: Handle) -> Option<&Element> {
        self.elements.get(&handle)
    }

    /// Records an added element; a parent of 0 marks a root. Adding a handle
    /// that is already known replaces it (the element moved). Returns false,
    /// and records nothing, for handle 0 or when the tree is full.
    pub fn add(&mut self, handle: Handle, parent: Handle, type_name: &str, name: &str) -> bool {
        if handle == 0 {
            return false;
        }
        if self.elements.len() >= MAX_ELEMENTS && !self.elements.contains_key(&handle) {
            return false;
        }
        let element = Element {
            parent: (parent != 0 && parent != handle).then_some(parent),
            type_name: type_name.into(),
            name: name.into(),
        };
        self.elements.insert(handle, element);
        true
    }

    /// Forgets an element and everything below it, and returns every handle
    /// that was forgotten. XAML Diagnostics may or may not report each
    /// descendant's removal separately; this way nothing stale stays behind
    /// either way.
    pub fn remove(&mut self, handle: Handle) -> Vec<Handle> {
        if !self.elements.contains_key(&handle) {
            return Vec::new();
        }
        let mut children: HashMap<Handle, Vec<Handle>> = HashMap::new();
        for (&child, element) in &self.elements {
            if let Some(parent) = element.parent {
                children.entry(parent).or_default().push(child);
            }
        }
        let mut removed = Vec::new();
        let mut pending = vec![handle];
        while let Some(next) = pending.pop() {
            if self.elements.remove(&next).is_some() {
                removed.push(next);
                if let Some(below) = children.remove(&next) {
                    pending.extend(below);
                }
            }
        }
        removed
    }

    /// The element's known ancestors, nearest first. Stops at the first
    /// unknown parent, and after `len()` steps so that a cycle in the
    /// reported parents cannot loop forever.
    pub fn ancestors(&self, handle: Handle) -> impl Iterator<Item = &Element> {
        let mut current = self.elements.get(&handle).and_then(|e| e.parent);
        let mut steps_left = self.elements.len();
        std::iter::from_fn(move || {
            if steps_left == 0 {
                return None;
            }
            steps_left -= 1;
            let element = self.elements.get(&current?)?;
            current = element.parent;
            Some(element)
        })
    }

    /// The element and its known ancestors as `Type#Name < Type#Name < …`,
    /// nearest first, for logs. At most `MAX_PATH_DEPTH` ancestors.
    pub fn path(&self, handle: Handle) -> String {
        let describe = |e: &Element| {
            if e.name.is_empty() {
                e.type_name.to_string()
            } else {
                format!("{}#{}", e.type_name, e.name)
            }
        };
        let Some(element) = self.elements.get(&handle) else {
            return format!("(unknown {handle:#x})");
        };
        std::iter::once(element)
            .chain(self.ancestors(handle).take(MAX_PATH_DEPTH))
            .map(describe)
            .collect::<Vec<_>>()
            .join(" < ")
    }

    /// Whether the element is one `rule` styles: right type and name, and
    /// the required ancestor among its known ancestors.
    pub fn matches(&self, handle: Handle, rule: &Rule) -> bool {
        let Some(element) = self.elements.get(&handle) else {
            return false;
        };
        rule.selects(&element.type_name, &element.name)
            && rule
                .ancestor
                .is_none_or(|name| self.ancestors(handle).any(|a| &*a.name == name))
    }

    /// Every element `rule` styles, in no particular order.
    pub fn matching(&self, rule: &Rule) -> Vec<Handle> {
        self.elements
            .keys()
            .copied()
            .filter(|&h| self.matches(h, rule))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::rules::{RECTANGLE_TYPE, TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME};

    const TASKBAR_BACKGROUND_NAME: &str = TASKBAR_BACKGROUND.name;

    const GRID: &str = "Windows.UI.Xaml.Controls.Grid";

    /// TaskbarFrame(1) > Grid#RootGrid(2) > TaskbarBackground(3) > Grid(4)
    /// > Rectangle#BackgroundFill(5), as in Windows 11 23H2/24H2.
    fn taskbar() -> ElementTree {
        let mut tree = ElementTree::new();
        tree.add(1, 0, "Taskbar.TaskbarFrame", TASKBAR_FRAME_NAME);
        tree.add(2, 1, GRID, "RootGrid");
        tree.add(3, 2, "Taskbar.TaskbarBackground", "BackgroundControl");
        tree.add(4, 3, GRID, "");
        tree.add(5, 4, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        tree
    }

    #[test]
    fn finds_the_taskbar_background() {
        let tree = taskbar();
        assert!(tree.matches(5, &TASKBAR_BACKGROUND));
        assert_eq!(tree.matching(&TASKBAR_BACKGROUND), vec![5]);
    }

    #[test]
    fn other_elements_are_not_the_background() {
        let tree = taskbar();
        for handle in 1..=4 {
            assert!(!tree.matches(handle, &TASKBAR_BACKGROUND));
        }
        assert!(!tree.matches(99, &TASKBAR_BACKGROUND));
    }

    #[test]
    fn background_fill_outside_a_taskbar_frame_is_ignored() {
        // The hover flyout has its own BackgroundFill rectangle.
        let mut tree = ElementTree::new();
        tree.add(
            10,
            0,
            "Taskbar.TaskbarBackground",
            "HoverFlyoutBackgroundControl",
        );
        tree.add(11, 10, GRID, "");
        tree.add(12, 11, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert!(!tree.matches(12, &TASKBAR_BACKGROUND));
        assert!(tree.matching(&TASKBAR_BACKGROUND).is_empty());
    }

    #[test]
    fn a_differently_typed_background_fill_is_ignored() {
        let mut tree = taskbar();
        tree.add(
            6,
            4,
            "Windows.UI.Xaml.Controls.Border",
            TASKBAR_BACKGROUND_NAME,
        );
        assert!(!tree.matches(6, &TASKBAR_BACKGROUND));
    }

    #[test]
    fn a_rectangle_added_before_its_ancestors_matches_once_they_arrive() {
        let mut tree = ElementTree::new();
        tree.add(5, 4, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert!(tree.matching(&TASKBAR_BACKGROUND).is_empty());
        tree.add(4, 1, GRID, "");
        tree.add(1, 0, "Taskbar.TaskbarFrame", TASKBAR_FRAME_NAME);
        assert_eq!(tree.matching(&TASKBAR_BACKGROUND), vec![5]);
    }

    #[test]
    fn one_background_per_taskbar() {
        let mut tree = taskbar();
        tree.add(21, 0, "Taskbar.TaskbarFrame", TASKBAR_FRAME_NAME);
        tree.add(25, 21, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        let mut found = tree.matching(&TASKBAR_BACKGROUND);
        found.sort_unstable();
        assert_eq!(found, vec![5, 25]);
    }

    #[test]
    fn path_lists_the_element_and_its_ancestors() {
        let tree = taskbar();
        assert_eq!(
            tree.path(4),
            "Windows.UI.Xaml.Controls.Grid < Taskbar.TaskbarBackground#BackgroundControl \
             < Windows.UI.Xaml.Controls.Grid#RootGrid < Taskbar.TaskbarFrame#TaskbarFrame"
        );
        assert_eq!(tree.path(99), "(unknown 0x63)");
    }

    #[test]
    fn path_is_bounded() {
        let mut tree = ElementTree::new();
        for handle in 1..=100 {
            tree.add(handle, handle - 1, GRID, "");
        }
        assert_eq!(tree.path(100).matches(" < ").count(), MAX_PATH_DEPTH);
    }

    #[test]
    fn removing_an_element_removes_its_subtree() {
        let mut tree = taskbar();
        tree.add(30, 0, GRID, "Unrelated");
        let mut removed = tree.remove(2);
        removed.sort_unstable();
        assert_eq!(removed, vec![2, 3, 4, 5]);
        assert_eq!(tree.len(), 2);
        assert!(tree.get(1).is_some() && tree.get(30).is_some());
        assert!(tree.matching(&TASKBAR_BACKGROUND).is_empty());
    }

    #[test]
    fn removing_an_unknown_element_does_nothing() {
        let mut tree = taskbar();
        assert!(tree.remove(42).is_empty());
        assert_eq!(tree.len(), 5);
    }

    #[test]
    fn handle_zero_is_rejected_and_a_self_parent_is_a_root() {
        let mut tree = ElementTree::new();
        assert!(!tree.add(0, 0, GRID, ""));
        assert!(tree.add(7, 7, GRID, ""));
        assert_eq!(tree.get(7).and_then(|e| e.parent), None);
    }

    #[test]
    fn re_adding_an_element_moves_it() {
        let mut tree = taskbar();
        tree.add(40, 0, GRID, "Elsewhere");
        tree.add(5, 40, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert!(!tree.matches(5, &TASKBAR_BACKGROUND));
        assert_eq!(tree.len(), 6);
    }

    #[test]
    fn a_parent_cycle_does_not_hang() {
        let mut tree = ElementTree::new();
        tree.add(1, 2, GRID, "");
        tree.add(2, 1, GRID, "");
        tree.add(3, 1, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert!(!tree.matches(3, &TASKBAR_BACKGROUND));
        assert!(tree.ancestors(3).count() <= tree.len());
    }

    #[test]
    fn the_tree_stops_growing_at_the_cap() {
        let mut tree = ElementTree::new();
        for handle in 1..=MAX_ELEMENTS as Handle {
            assert!(tree.add(handle, 0, GRID, ""));
        }
        assert!(!tree.add(MAX_ELEMENTS as Handle + 1, 0, GRID, ""));
        // Known elements can still be updated.
        assert!(tree.add(1, 0, GRID, "renamed"));
        assert_eq!(tree.len(), MAX_ELEMENTS);
    }
}
