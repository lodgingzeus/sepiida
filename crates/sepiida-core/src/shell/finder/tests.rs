use super::*;
use crate::shell::rules::{
    Effect, Name, Paint, RECTANGLE_TYPE, Role, Step, TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME,
    TASKBAR_RULES,
};

const GRID: &str = "Windows.UI.Xaml.Controls.Grid";
const FRAME_TYPE: &str = "Taskbar.TaskbarFrame";
const BACKGROUND_TYPE: &str = "Taskbar.TaskbarBackground";
const FILL: &str = "BackgroundFill";

/// The taskbar's colour rule alone; the look rules on the same
/// rectangle have a test of their own.
static FILL_ONLY: [Rule; 1] = [TASKBAR_BACKGROUND];

fn taskbar_finder() -> Finder {
    Finder::new(&FILL_ONLY)
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
    assert!(finder.pending_dropped_count() > 0);
}

static TWO_RULES: [Rule; 2] = [
    TASKBAR_BACKGROUND,
    Rule {
        label: "anywhere",
        type_name: "Windows.UI.Xaml.Controls.Border",
        name: Name::Is("Acrylic"),
        ancestor: None,
        parent: None,
        chain: &[],
        position: None,
        conditions: &[],
        material: None,
        property: "Background",
        effect: Effect::Color {
            paint: Paint::Tint,
            role: Role::Accent,
        },
    },
];

#[test]
fn a_rule_without_an_ancestor_matches_at_once_and_ids_follow_the_rule_set() {
    let mut finder = Finder::new(&TWO_RULES);
    let added = finder.added(8, 0, "Windows.UI.Xaml.Controls.Border", "Acrylic");
    assert_eq!(added.matched, vec![Match { handle: 8, rule: 1 }]);
    assert!(matches!(
        finder.rule(1).map(|r| r.effect),
        Some(Effect::Color {
            role: Role::Accent,
            ..
        })
    ));
    assert_eq!(finder.rule(2), None);
    finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
    assert_eq!(
        finder.added(5, 1, RECTANGLE_TYPE, FILL).matched,
        vec![hit(5)]
    );
}

#[test]
fn every_rule_on_one_element_matches_it() {
    let mut finder = Finder::new(TASKBAR_RULES);
    finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
    let on_fill: Vec<RuleId> = TASKBAR_RULES
        .iter()
        .enumerate()
        .filter(|(_, r)| r.name == Name::Is(FILL))
        .map(|(id, _)| id)
        .collect();
    assert!(on_fill.len() > 1);
    let matched: Vec<RuleId> = finder
        .added(5, 1, RECTANGLE_TYPE, FILL)
        .matched
        .into_iter()
        .map(|m| m.rule)
        .collect();
    assert_eq!(matched, on_fill);
}

static UNNAMED_GRID: [Rule; 1] = [Rule {
    label: "unnamed grid",
    type_name: GRID,
    name: Name::Is(""),
    ancestor: Some("BackgroundControl"),
    parent: None,
    chain: &[],
    position: None,
    conditions: &[],
    material: None,
    property: "Background",
    effect: Effect::Color {
        paint: Paint::Tint,
        role: Role::Surface,
    },
}];

#[test]
fn unnamed_candidates_with_a_complete_chain_do_not_crowd_out_pending_ones() {
    let mut finder = Finder::new(&UNNAMED_GRID);
    finder.added(1, 0, FRAME_TYPE, TASKBAR_FRAME_NAME);
    for handle in 10..(10 + MAX_PENDING as Handle * 2) {
        finder.added(handle, 1, GRID, "");
    }
    assert!(finder.pending.is_empty());
    // The real one arrives before its ancestor, and still matches.
    finder.added(500, 400, GRID, "");
    assert_eq!(finder.pending.len(), 1);
    let added = finder.added(400, 1, BACKGROUND_TYPE, "BackgroundControl");
    assert_eq!(
        added.matched,
        vec![Match {
            handle: 500,
            rule: 0
        }]
    );
    assert!(finder.pending.is_empty());
}

#[test]
fn a_pending_candidate_is_dropped_once_its_chain_completes_without_a_match() {
    let mut finder = Finder::new(&UNNAMED_GRID);
    finder.added(500, 400, GRID, "");
    assert_eq!(finder.pending.len(), 1);
    finder.added(400, 0, BACKGROUND_TYPE, "Elsewhere");
    assert!(finder.pending.is_empty());
    assert_eq!(finder.matched_count(), 0);
}

static UNDER_L1: [Step; 1] = [Step::Element {
    type_name: GRID,
    name: Name::Is("L1Grid"),
    position: None,
}];
static ANY_BORDER_UNDER_L1: [Rule; 1] = [Rule {
    label: "Grid#L1Grid > Border",
    type_name: "Windows.UI.Xaml.Controls.Border",
    name: Name::Any,
    ancestor: None,
    parent: None,
    chain: &UNDER_L1,
    position: None,
    conditions: &[],
    material: None,
    property: "Background",
    effect: Effect::Color {
        paint: Paint::Tint,
        role: Role::Surface,
    },
}];

/// A theme rule on a common type (`Border`) under a named parent: the
/// many Borders whose parent is known and different are ruled out at
/// once, not kept pending, so a real target arriving later is found.
#[test]
fn candidates_ruled_out_by_known_parents_are_not_kept() {
    const BORDER: &str = "Windows.UI.Xaml.Controls.Border";
    let mut finder = Finder::new(&ANY_BORDER_UNDER_L1);
    // No root is ever reported (as in the sandboxed hosts): the top
    // grid's parent stays unknown.
    finder.added(1, 9999, GRID, "RootGrid");
    for handle in 10..(10 + 2 * MAX_PENDING as Handle) {
        finder.added(handle, 1, BORDER, "");
    }
    assert!(finder.pending.is_empty());
    assert_eq!(finder.pending_dropped_count(), 0);
    finder.added(2, 1, GRID, "L1Grid");
    assert_eq!(finder.added(3, 2, BORDER, "").matched.len(), 1);
    // A Border whose parent is not reported yet waits for it.
    assert!(finder.added(5, 4, BORDER, "").matched.is_empty());
    assert_eq!(finder.pending.len(), 1);
    assert_eq!(finder.added(4, 1, GRID, "L1Grid").matched.len(), 1);
    assert!(finder.pending.is_empty());
}

#[test]
fn the_child_index_of_a_positioned_type_is_reported() {
    static CHAIN: [Step; 1] = [Step::Element {
        type_name: GRID,
        name: Name::Any,
        position: Some(1),
    }];
    static RULES: [Rule; 1] = [Rule {
        chain: &CHAIN,
        ..ANY_BORDER_UNDER_L1[0]
    }];
    let mut finder = Finder::new(&RULES);
    assert_eq!(finder.added_at(1, 0, None, GRID, "").positioned, None);
    assert_eq!(finder.added_at(2, 1, Some(1), GRID, "").positioned, Some(1));
    let border = "Windows.UI.Xaml.Controls.Border";
    let added = finder.added_at(3, 2, Some(0), border, "");
    assert_eq!(added.positioned, None);
    assert_eq!(added.matched.len(), 1);
}
