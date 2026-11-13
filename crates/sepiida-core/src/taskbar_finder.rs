//! Finds taskbar background rectangles in the stream of visual tree changes
//! XAML Diagnostics reports.
//!
//! Elements can arrive in any order: a `BackgroundFill` rectangle may be
//! reported before the ancestors that make it a taskbar's. Such rectangles
//! are kept as pending and re-tested after every later addition.

use std::collections::HashSet;

use crate::xaml_tree::{
    ElementTree, Handle, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME, TASKBAR_FRAME_NAME,
};

/// Unmatched `BackgroundFill` rectangles re-tested on every addition. One
/// per hover flyout is normal; the cap only bounds the per-callback cost.
pub const MAX_PENDING: usize = 64;

/// What one addition revealed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Added {
    /// Taskbar backgrounds that became known with this addition.
    pub matched: Vec<Handle>,
    /// The first `TaskbarFrame` of this tree was added.
    pub first_frame: bool,
    /// A `BackgroundFill` rectangle that is not (yet) under a taskbar
    /// frame, with its path, for the log.
    pub near_miss: Option<String>,
}

#[derive(Debug, Default)]
pub struct TaskbarFinder {
    tree: ElementTree,
    pending: Vec<Handle>,
    matched: HashSet<Handle>,
    saw_frame: bool,
}

impl TaskbarFinder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn element_count(&self) -> usize {
        self.tree.len()
    }

    pub fn saw_frame(&self) -> bool {
        self.saw_frame
    }

    pub fn matched_count(&self) -> usize {
        self.matched.len()
    }

    pub fn added(&mut self, handle: Handle, parent: Handle, type_name: &str, name: &str) -> Added {
        let mut result = Added::default();
        if !self.tree.add(handle, parent, type_name, name) {
            return result;
        }
        if name == TASKBAR_FRAME_NAME && !self.saw_frame {
            self.saw_frame = true;
            result.first_frame = true;
        }
        if name == TASKBAR_BACKGROUND_NAME
            && type_name == RECTANGLE_TYPE
            && !self.matched.contains(&handle)
            && !self.pending.contains(&handle)
        {
            if self.tree.is_taskbar_background(handle) {
                self.matched.insert(handle);
                result.matched.push(handle);
            } else {
                result.near_miss = Some(self.tree.path(handle));
                if self.pending.len() < MAX_PENDING {
                    self.pending.push(handle);
                }
            }
        }
        if !self.pending.is_empty() {
            let tree = &self.tree;
            let (now_matched, still_pending): (Vec<Handle>, Vec<Handle>) = self
                .pending
                .iter()
                .partition(|&&h| tree.is_taskbar_background(h));
            self.pending = still_pending;
            for h in now_matched {
                if self.matched.insert(h) {
                    result.matched.push(h);
                }
            }
        }
        result
    }

    /// Forgets an element and its subtree. Returns the taskbar backgrounds
    /// that went with it.
    pub fn removed(&mut self, handle: Handle) -> Vec<Handle> {
        let removed = self.tree.remove(handle);
        if removed.is_empty() {
            return removed;
        }
        let removed: HashSet<Handle> = removed.into_iter().collect();
        self.pending.retain(|h| !removed.contains(h));
        let gone: Vec<Handle> = self
            .matched
            .iter()
            .copied()
            .filter(|h| removed.contains(h))
            .collect();
        for h in &gone {
            self.matched.remove(h);
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
    const FRAME_TYPE: &str = "Taskbar.TaskbarFrame";
    const BACKGROUND_TYPE: &str = "Taskbar.TaskbarBackground";

    #[test]
    fn parents_first_matches_on_the_rectangle() {
        let mut finder = TaskbarFinder::new();
        assert!(
            finder
                .added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME)
                .first_frame
        );
        finder.added(2, 1, GRID, "RootGrid");
        finder.added(3, 2, BACKGROUND_TYPE, "BackgroundControl");
        finder.added(4, 3, GRID, "");
        let added = finder.added(5, 4, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert_eq!(added.matched, vec![5]);
        assert_eq!(added.near_miss, None);
        assert!(finder.saw_frame());
        assert_eq!(finder.matched_count(), 1);
    }

    #[test]
    fn an_intermediate_ancestor_arriving_last_still_matches() {
        let mut finder = TaskbarFinder::new();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        let early = finder.added(5, 4, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        assert!(early.matched.is_empty());
        assert!(early.near_miss.is_some());
        finder.added(4, 3, GRID, "");
        // Grid 3 links 4 to the frame; nothing is named specially.
        let late = finder.added(3, 1, BACKGROUND_TYPE, "BackgroundControl");
        assert_eq!(late.matched, vec![5]);
    }

    #[test]
    fn a_match_is_reported_once() {
        let mut finder = TaskbarFinder::new();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        assert_eq!(
            finder
                .added(5, 1, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME)
                .matched,
            vec![5]
        );
        assert!(
            finder
                .added(5, 1, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME)
                .matched
                .is_empty()
        );
        assert!(finder.added(6, 1, GRID, "").matched.is_empty());
        assert!(
            !finder
                .added(7, 0, FRAME_TYPE, TASKBAR_FRAME_NAME)
                .first_frame
        );
    }

    #[test]
    fn the_hover_flyout_is_a_near_miss_that_never_matches() {
        let mut finder = TaskbarFinder::new();
        finder.added(10, 0, BACKGROUND_TYPE, "HoverFlyoutBackgroundControl");
        let added = finder.added(11, 10, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
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
    fn removal_reports_the_backgrounds_that_went() {
        let mut finder = TaskbarFinder::new();
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        finder.added(2, 1, GRID, "");
        finder.added(5, 2, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        finder.added(9, 0, GRID, "");
        assert!(finder.removed(9).is_empty());
        assert_eq!(finder.removed(1), vec![5]);
        assert_eq!(finder.matched_count(), 0);
        assert!(finder.removed(1).is_empty());
    }

    #[test]
    fn removed_pending_rectangles_are_not_rechecked() {
        let mut finder = TaskbarFinder::new();
        finder.added(5, 4, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
        finder.removed(5);
        finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
        // Handle 5 reused by an unrelated element under the frame.
        assert!(finder.added(5, 1, GRID, "").matched.is_empty());
        assert_eq!(finder.matched_count(), 0);
    }

    #[test]
    fn pending_is_capped() {
        let mut finder = TaskbarFinder::new();
        for handle in 1..=(MAX_PENDING as Handle + 10) {
            let added = finder.added(handle, 1000, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
            assert!(added.near_miss.is_some());
        }
        assert_eq!(finder.pending.len(), MAX_PENDING);
    }
}
