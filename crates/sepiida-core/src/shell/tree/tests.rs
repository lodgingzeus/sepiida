use super::*;
use crate::shell::rules::{Name, RECTANGLE_TYPE, TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME};

const TASKBAR_BACKGROUND_NAME: &str = "BackgroundFill";

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

#[test]
fn a_chain_reaches_the_root_only_once_every_parent_is_known() {
    let mut tree = ElementTree::new();
    tree.add(3, 2, "C", "");
    assert!(!tree.reaches_root(3));
    tree.add(2, 1, "B", "");
    assert!(!tree.reaches_root(3));
    tree.add(1, 0, "A", "");
    assert!(tree.reaches_root(3));
    assert!(tree.reaches_root(1));
    assert!(!tree.reaches_root(99));
    // A cycle never counts as complete.
    tree.add(10, 11, "X", "");
    tree.add(11, 10, "Y", "");
    assert!(!tree.reaches_root(10));
}

#[test]
fn a_parent_rule_needs_the_direct_parent_by_name() {
    let rule = Rule {
        label: "frame grid",
        type_name: GRID,
        name: Name::Is("RootGrid"),
        ancestor: None,
        parent: Some(TASKBAR_FRAME_NAME),
        ..TASKBAR_BACKGROUND
    };
    let tree = taskbar();
    assert!(tree.matches(2, &rule));
    let mut tree = taskbar();
    // A RootGrid deeper down, under the frame but not its child.
    tree.add(20, 4, GRID, "RootGrid");
    assert!(!tree.matches(20, &rule));
    // Parent not known yet.
    tree.add(30, 29, GRID, "RootGrid");
    assert!(!tree.matches(30, &rule));
}

/// `Grid > Button[2] > Border[1]`, as a Windhawk theme writes it.
fn second_button_border() -> Rule {
    static CHAIN: [Step; 2] = [
        Step::Element {
            type_name: "Windows.UI.Xaml.Controls.Button",
            name: Name::Any,
            position: Some(1),
        },
        Step::Element {
            type_name: GRID,
            name: Name::Any,
            position: None,
        },
    ];
    Rule {
        type_name: "Windows.UI.Xaml.Controls.Border",
        name: Name::Any,
        ancestor: None,
        parent: None,
        chain: &CHAIN,
        position: Some(0),
        conditions: &[],
        material: None,
        ..TASKBAR_BACKGROUND
    }
}

#[test]
fn a_position_picks_one_child_of_the_parent() {
    const BUTTON: &str = "Windows.UI.Xaml.Controls.Button";
    const BORDER: &str = "Windows.UI.Xaml.Controls.Border";
    let mut tree = ElementTree::new();
    tree.add(1, 0, GRID, "");
    tree.add_at(2, 1, Some(0), BUTTON, "");
    tree.add_at(3, 1, Some(1), BUTTON, "");
    tree.add_at(4, 2, Some(0), BORDER, "");
    tree.add_at(5, 3, Some(0), BORDER, "");
    tree.add_at(6, 3, Some(1), BORDER, "");
    // An element whose index was not reported never matches a position.
    tree.add(7, 3, BORDER, "");
    assert_eq!(tree.matching(&second_button_border()), vec![5]);
}
