//! The flyouts in ShellExperienceHost, with the shipped rules: notification
//! cards get a solid fill of the raised surface colour that is painted
//! again when Windows puts its own brush back (but not after a restore),
//! and round corners when they appear while the look is on; a layer the
//! theme clears turns transparent with the look and comes back.

use sepiida_core::{
    Color,
    shell::{
        hosts::Host,
        protocol::Command,
        rules::{BORDER_TYPE, GRID_TYPE, Role},
    },
};

use super::{add, fake::*, new_tap, send, serial, wait_for_callback, wait_until};
use crate::xamlom::*;

const PLATING: InstanceHandle = 40;
/// The plating's parent: WindowGlass selects `Grid > Border#ItemOpaquePlating`.
const CARD_GRID: InstanceHandle = 39;
/// Windows' shared card brush, and the one it puts back after a switch.
const WINDOWS_CARD: InstanceHandle = 0x500;
const WINDOWS_CARD_AFTER_SWITCH: InstanceHandle = 0x501;
const BRUSH: &str = "Windows.UI.Xaml.Media.SolidColorBrush";

fn attach_to_notifications() -> (std::sync::Arc<Shared>, IVisualTreeServiceCallback2) {
    crate::host::tests::pretend(Host::ShellExperience);
    let (shared, site) = new_fake();
    shared
        .values
        .lock()
        .unwrap()
        .insert((PLATING, BACKGROUND_INDEX), WINDOWS_CARD);
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    (shared, callback)
}

fn detach(shared: &Shared) {
    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

/// Waits out the re-check interval, then reports tree activity.
fn tree_activity(callback: &IVisualTreeServiceCallback2, handle: InstanceHandle) {
    std::thread::sleep(std::time::Duration::from_millis(300));
    add(callback, handle, PLATING, GRID_TYPE, "");
}

#[test]
fn a_card_brush_windows_puts_back_is_painted_again_but_not_after_restore() {
    let _serial = serial();
    let (shared, callback) = attach_to_notifications();
    add(&callback, CARD_GRID, 0, GRID_TYPE, "");
    add(
        &callback,
        PLATING,
        CARD_GRID,
        BORDER_TYPE,
        "ItemOpaquePlating",
    );
    send(Command::SetColor(
        Role::SurfaceRaised,
        Color::new(0x30, 0x40, 0x50),
    ));
    assert!(shared.has(&format!("CreateInstance {BRUSH} null -> 0x1000")));
    let ours = format!("SetProperty {PLATING:#x} {BACKGROUND_INDEX} 0x1000");
    assert_eq!(shared.count(&ours), 1);

    // A light/dark switch: Windows gives the card its theme brush again.
    shared
        .values
        .lock()
        .unwrap()
        .insert((PLATING, BACKGROUND_INDEX), WINDOWS_CARD_AFTER_SWITCH);
    tree_activity(&callback, 41);
    assert_eq!(shared.count(&ours), 2);

    // Restore brings back the brush Windows put there last, and later tree
    // activity paints nothing.
    send(Command::Restore);
    assert!(shared.has(&format!(
        "SetProperty {PLATING:#x} {BACKGROUND_INDEX} {WINDOWS_CARD_AFTER_SWITCH:#x}"
    )));
    tree_activity(&callback, 42);
    assert_eq!(shared.count(&ours), 2);
    detach(&shared);
}

#[test]
fn a_card_that_appears_while_the_look_is_on_gets_round_corners() {
    let _serial = serial();
    let (shared, callback) = attach_to_notifications();
    add(&callback, 30, 0, GRID_TYPE, "NotificationCenterGrid");
    send(Command::SetLook(true));
    let corners = format!("SetProperty {PLATING:#x} {CORNER_RADIUS_INDEX} ");
    assert!(!shared.calls().iter().any(|c| c.starts_with(&corners)));

    add(&callback, PLATING, 30, BORDER_TYPE, "ItemOpaquePlating");
    assert!(
        shared
            .calls()
            .iter()
            .any(|c| c.starts_with("CreateInstance Windows.UI.Xaml.CornerRadius 12,12,12,12"))
    );
    assert!(shared.calls().iter().any(|c| c.starts_with(&corners)));

    send(Command::SetLook(false));
    assert!(shared.has(&format!("ClearProperty {PLATING:#x} {CORNER_RADIUS_INDEX}")));
    detach(&shared);
}

#[test]
fn the_look_makes_the_toast_layer_transparent_and_puts_it_back() {
    let _serial = serial();
    let (shared, callback) = attach_to_notifications();
    const LAYER: InstanceHandle = 50;
    add(
        &callback,
        LAYER,
        0,
        "ActionCenter.FlexibleToastView",
        "FlexibleNormalToastView",
    );
    let set = format!("SetProperty {LAYER:#x} {BACKGROUND_INDEX} ");
    assert!(!shared.calls().iter().any(|c| c.starts_with(&set)));

    send(Command::SetLook(true));
    assert!(shared.has(&format!("CreateInstance {BRUSH} null -> 0x1000")));
    assert!(shared.has("CreateInstance Windows.UI.Color #00000000 -> 0x1001"));
    assert!(shared.has(&format!("{set}0x1000")));

    // The fake's Background comes from a style: turning the look off
    // clears ours.
    send(Command::SetLook(false));
    assert!(shared.has(&format!("ClearProperty {LAYER:#x} {BACKGROUND_INDEX}")));
    // On again: the same brush, not a new one.
    send(Command::SetLook(true));
    assert_eq!(shared.count(&format!("{set}0x1000")), 2);
    assert!(!shared.has(&format!("CreateInstance {BRUSH} null -> 0x1002")));
    detach(&shared);
}

#[test]
fn a_look_brush_windows_replaces_is_set_again_and_its_brush_restored() {
    let _serial = serial();
    let (shared, callback) = attach_to_notifications();
    const LAYER: InstanceHandle = 60;
    const WINDOWS_BRUSH: InstanceHandle = 0x777;
    add(
        &callback,
        LAYER,
        0,
        "ActionCenter.FlexibleToastView",
        "FlexibleNormalToastView",
    );
    send(Command::SetLook(true));
    let ours = format!("SetProperty {LAYER:#x} {BACKGROUND_INDEX} 0x1000");
    assert_eq!(shared.count(&ours), 1);

    // The flyout reopens with Windows' own brush on the layer.
    shared
        .values
        .lock()
        .unwrap()
        .insert((LAYER, BACKGROUND_INDEX), WINDOWS_BRUSH);
    std::thread::sleep(std::time::Duration::from_millis(300));
    add(&callback, 61, LAYER, GRID_TYPE, "");
    assert_eq!(shared.count(&ours), 2);

    // Look off puts Windows' newer brush back, not a cleared value.
    send(Command::SetLook(false));
    assert!(shared.has(&format!(
        "SetProperty {LAYER:#x} {BACKGROUND_INDEX} {WINDOWS_BRUSH:#x}"
    )));
    detach(&shared);
}
