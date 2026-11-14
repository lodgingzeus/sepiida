//! Tinted paint in a sandboxed host (Start): Windows' acrylic brush keeps
//! its place and gets our tint; overlays are only logged; the original
//! tint comes back on restore, even once the element has left the tree.

use sepiida_core::{
    Color,
    shell::{
        hosts::Host,
        protocol::Command,
        rules::{BORDER_TYPE, GRID_TYPE, Role},
        tint::Opacity,
    },
};
use windows::core::BSTR;

use super::{add, control_window, fake::*, new_tap, send, serial, wait_for_callback, wait_until};
use crate::xamlom::*;

const START_BACKGROUND: InstanceHandle = 20;
const OVERLAY: InstanceHandle = 21;

fn remove(callback: &IVisualTreeServiceCallback2, handle: u64) {
    let relation = ParentChildRelation {
        Parent: 0,
        Child: handle,
        ChildIndex: 0,
    };
    let element = VisualElement {
        Handle: handle,
        SrcInfo: empty_source_info(),
        Type: BSTR::new(),
        Name: BSTR::new(),
        NumChildren: 0,
    };
    // SAFETY: plain COM call; the element's strings move to the callee.
    let hr = unsafe { callback.OnVisualTreeChange(relation, element, VisualMutationType::REMOVE) };
    hr.unwrap();
}

/// Attaches as `host` with `START_BACKGROUND`'s Background set to `value`.
fn attach_as(
    host: Host,
    value: InstanceHandle,
) -> (std::sync::Arc<Shared>, IVisualTreeServiceCallback2) {
    crate::host::tests::pretend(host);
    let (shared, site) = new_fake();
    shared
        .values
        .lock()
        .unwrap()
        .insert((START_BACKGROUND, BACKGROUND_INDEX), value);
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    (shared, callback)
}

fn detach(shared: &Shared) {
    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
    assert!(control_window().is_none());
}

fn nothing_created_or_replaced(shared: &Shared) -> bool {
    !shared
        .calls()
        .iter()
        .any(|c| c.starts_with("CreateInstance") || c.starts_with("SetProperty"))
}

#[test]
fn start_acrylic_is_tinted_and_restored_after_its_element_left() {
    let _serial = serial();
    let (shared, callback) = attach_as(Host::StartMenu, ACRYLIC);
    add(&callback, START_BACKGROUND, 0, BORDER_TYPE, "AcrylicBorder");
    add(&callback, OVERLAY, 0, BORDER_TYPE, "AcrylicOverlay");
    // The overlay's chain is read (logged), and nothing else happens to it.
    assert!(shared.has(&format!("GetPropertyValuesChain {OVERLAY:#x}")));

    // Our colour, Windows' alpha and opacity.
    send(Command::SetColor(
        Role::Surface,
        Color::new(0x33, 0x66, 0x99),
    ));
    assert!(shared.has("put_TintColor #CC336699"));
    assert!(shared.has("put_TintOpacity 0.5"));
    // The brush's own properties are logged once, from its chain.
    assert_eq!(
        shared.count(&format!("GetPropertyValuesChain {ACRYLIC:#x}")),
        1
    );

    // Opacity, then back to Windows' own.
    send(Command::SetTintOpacity(
        Role::Surface,
        Opacity::from_percent(80),
    ));
    assert!(shared.has("put_TintOpacity 0.8"));
    send(Command::SetTintOpacity(Role::Surface, None));
    assert_eq!(shared.count("put_TintOpacity 0.5"), 2);

    // The element leaves the tree; restore still puts the tint back.
    remove(&callback, START_BACKGROUND);
    send(Command::Restore);
    assert!(shared.has("put_TintColor #CC202020"));
    assert_eq!(shared.current_tint(), WINDOWS_TINT);
    assert!(nothing_created_or_replaced(&shared));

    detach(&shared);
}

#[test]
fn a_tint_windows_changed_is_the_one_restored() {
    let _serial = serial();
    let (shared, callback) = attach_as(Host::StartMenu, ACRYLIC);
    add(&callback, START_BACKGROUND, 0, BORDER_TYPE, "AcrylicBorder");
    send(Command::SetColor(Role::Surface, Color::new(1, 2, 3)));
    assert!(shared.has("put_TintColor #CC010203"));

    // A theme change: Windows sets its own tint on the same brush.
    let light = windows::UI::Color {
        A: 0xff,
        R: 0xf3,
        G: 0xf3,
        B: 0xf3,
    };
    *shared.tint.lock().unwrap() = Some((light, 0.6));
    send(Command::SetColor(Role::Surface, Color::new(4, 5, 6)));
    assert!(shared.has("put_TintColor #FF040506"));
    send(Command::Restore);
    assert!(shared.has("put_TintColor #FFF3F3F3"));
    assert_eq!(shared.current_tint(), (light, 0.6));

    detach(&shared);
}

#[test]
fn a_value_that_is_not_acrylic_is_left_alone() {
    let _serial = serial();
    // The original fill stands in for any brush that is not acrylic.
    let (shared, callback) = attach_as(Host::ShellExperience, ORIGINAL_FILL);
    add(
        &callback,
        START_BACKGROUND,
        0,
        GRID_TYPE,
        "NotificationCenterGrid",
    );
    send(Command::SetColor(Role::Surface, Color::new(1, 2, 3)));
    send(Command::Restore);
    assert!(nothing_created_or_replaced(&shared));
    assert!(!shared.calls().iter().any(|c| c.starts_with("put_Tint")));
    detach(&shared);
}

#[test]
fn each_host_only_styles_its_own_surfaces() {
    let _serial = serial();
    // Start's rules do not include the taskbar.
    let (shared, callback) = attach_as(Host::StartMenu, ACRYLIC);
    add(
        &callback,
        RECT_HANDLE,
        0,
        "Windows.UI.Xaml.Shapes.Rectangle",
        "BackgroundFill",
    );
    send(Command::SetColor(Role::Surface, Color::new(1, 2, 3)));
    assert!(!shared.has(&format!("GetPropertyValuesChain {RECT_HANDLE:#x}")));
    assert!(nothing_created_or_replaced(&shared));
    detach(&shared);
}
