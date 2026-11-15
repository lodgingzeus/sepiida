//! Look rules on the taskbar: nothing changes until the look is turned on;
//! then each value is created from its string and set, except where a
//! visual state drives the property; turning the look off recreates a
//! local original and clears the rest.

use sepiida_core::{
    Color,
    shell::{
        layout::{Value, Visibility},
        protocol::Command,
        rules::{Effect, Material, Name, RECTANGLE_TYPE, Role, Rule, TASKBAR_FRAME_NAME},
    },
};

use super::{add_taskbar, fake::*, new_tap, send, serial, wait_for_callback, wait_until};

const fn look(property: &'static str, value: Value) -> Rule {
    Rule {
        label: "look",
        type_name: RECTANGLE_TYPE,
        name: Name::Is("BackgroundFill"),
        ancestor: Some(TASKBAR_FRAME_NAME),
        parent: None,
        chain: &[],
        position: None,
        conditions: &[],
        material: None,
        property,
        effect: Effect::Set(value),
    }
}

static LOOK: [Rule; 4] = [
    look("Margin", Value::Thickness([8.0, 4.0, 8.0, 4.0])),
    look("RadiusX", Value::Length(8.0)),
    look("RadiusY", Value::Length(8.0)),
    look("Visibility", Value::Visibility(Visibility::Collapsed)),
];

#[test]
fn the_look_is_written_on_demand_and_put_back() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &LOOK);
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    let created = |shared: &Shared| {
        shared
            .calls()
            .iter()
            .filter(|c| c.starts_with("CreateInstance"))
            .count()
    };
    assert_eq!(created(&shared), 0, "nothing before the look is on");

    send(Command::SetLook(true));
    assert!(shared.has("CreateInstance Windows.UI.Xaml.Thickness 8,4,8,4 -> 0x1000"));
    assert!(shared.has(&format!("SetProperty 0x5 {MARGIN_INDEX} 0x1000")));
    assert!(shared.has("CreateInstance Windows.Foundation.Double 8 -> 0x1001"));
    assert!(shared.has(&format!("SetProperty 0x5 {RADIUS_X_INDEX} 0x1001")));
    assert!(shared.has("CreateInstance Windows.UI.Xaml.Visibility Collapsed -> 0x1002"));
    assert!(shared.has(&format!("SetProperty 0x5 {VISIBILITY_INDEX} 0x1002")));
    // A visual state drives RadiusY in the fake: left alone.
    assert!(
        !shared
            .calls()
            .iter()
            .any(|c| c.starts_with(&format!("SetProperty 0x5 {RADIUS_Y_INDEX} ")))
    );
    // Turning it on again writes nothing new.
    send(Command::SetLook(true));
    assert_eq!(created(&shared), 3);

    send(Command::SetLook(false));
    assert!(shared.has(&format!(
        "CreateInstance Windows.UI.Xaml.Thickness {LOCAL_MARGIN} -> 0x1003"
    )));
    assert!(shared.has(&format!("SetProperty 0x5 {MARGIN_INDEX} 0x1003")));
    assert!(shared.has(&format!("ClearProperty 0x5 {RADIUS_X_INDEX}")));
    assert!(shared.has(&format!("ClearProperty 0x5 {VISIBILITY_INDEX}")));
    assert!(!shared.has(&format!("ClearProperty 0x5 {RADIUS_Y_INDEX}")));

    // On again, then a restore puts everything back and leaves it off.
    send(Command::SetLook(true));
    assert_eq!(created(&shared), 7);
    send(Command::Restore);
    assert_eq!(
        shared.count(&format!("ClearProperty 0x5 {RADIUS_X_INDEX}")),
        2
    );
    send(Command::Restore);
    assert_eq!(
        shared.count(&format!("ClearProperty 0x5 {RADIUS_X_INDEX}")),
        2
    );

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

#[test]
fn a_value_an_animation_held_is_written_once_it_lets_go() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &LOOK);
    let (shared, site) = new_fake();
    shared
        .radius_x_animated
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    let radius_x_set = |shared: &Shared| {
        shared
            .calls()
            .iter()
            .any(|c| c.starts_with(&format!("SetProperty 0x5 {RADIUS_X_INDEX} ")))
    };

    send(Command::SetLook(true));
    assert!(shared.has(&format!("SetProperty 0x5 {MARGIN_INDEX} 0x1000")));
    assert!(!radius_x_set(&shared), "held by the animation");
    // Still held: another "look on" tries again and writes nothing.
    send(Command::SetLook(true));
    assert!(!radius_x_set(&shared));

    // The animation lets go; the next "look on" (the agent sends one after
    // a surface is shown) writes it.
    shared
        .radius_x_animated
        .store(false, std::sync::atomic::Ordering::SeqCst);
    send(Command::SetLook(true));
    assert!(radius_x_set(&shared));
    // Off puts it back like any other value.
    send(Command::SetLook(false));
    assert!(shared.has(&format!("ClearProperty 0x5 {RADIUS_X_INDEX}")));
    // A visual state is not an animation: RadiusY is never retried.
    assert!(
        !shared
            .calls()
            .iter()
            .any(|c| c.starts_with(&format!("SetProperty 0x5 {RADIUS_Y_INDEX} ")))
    );

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

static GLASS: [Rule; 1] = [look("Background", Value::Acrylic(Role::Surface))];

#[test]
fn an_acrylic_value_waits_for_its_colour_and_follows_it() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &GLASS);
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    let set = format!("SetProperty 0x5 {BACKGROUND_INDEX} {LIVE_FILL:#x}");

    send(Command::SetLook(true));
    assert!(!shared.has(&set), "no colour yet");

    send(Command::SetColor(
        Role::Surface,
        Color::new(0x10, 0x20, 0x30),
    ));
    assert!(shared.has(&format!(
        "CreateInstance Windows.UI.Xaml.Media.AcrylicBrush null -> {ACRYLIC:#x}"
    )));
    assert!(shared.has(&set));
    let tint = |shared: &Shared| shared.tint.lock().unwrap().expect("tinted");
    let (color, opacity) = tint(&shared);
    assert_eq!(
        (color.A, color.R, color.G, color.B),
        (0xff, 0x10, 0x20, 0x30)
    );
    assert!((opacity - 0.5).abs() < 1e-9);

    // A new colour re-tints the same brush; nothing is set again.
    send(Command::SetColor(
        Role::Surface,
        Color::new(0x40, 0x50, 0x60),
    ));
    assert_eq!(tint(&shared).0.R, 0x40);
    assert_eq!(shared.count(&set), 1);

    send(Command::SetLook(false));
    assert!(shared.has(&format!("ClearProperty 0x5 {BACKGROUND_INDEX}")));

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

static TWO_ON_ONE: [Rule; 2] = [
    look("RadiusX", Value::Length(8.0)),
    look("RadiusX", Value::Length(12.0)),
];

#[test]
fn a_later_rule_on_the_same_property_wins() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &TWO_ON_ONE);
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);

    send(Command::SetLook(true));
    assert!(shared.has("CreateInstance Windows.Foundation.Double 12 -> 0x1000"));
    assert!(shared.has(&format!("SetProperty 0x5 {RADIUS_X_INDEX} 0x1000")));
    assert!(!shared.has("CreateInstance Windows.Foundation.Double 8 -> 0x1000"));
    assert_eq!(
        shared
            .calls()
            .iter()
            .filter(|c| c.starts_with(&format!("SetProperty 0x5 {RADIUS_X_INDEX} ")))
            .count(),
        1
    );

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

static BY_MATERIAL: [Rule; 2] = [
    Rule {
        material: Some(Material::Glass),
        ..look("RadiusX", Value::Length(8.0))
    },
    Rule {
        material: Some(Material::Solid),
        ..look("RadiusX", Value::Length(12.0))
    },
];

#[test]
fn a_material_switch_puts_one_value_back_before_writing_the_other() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &BY_MATERIAL);
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    let made = |shared: &Shared, value: &str| {
        shared
            .calls()
            .iter()
            .filter(|c| {
                c.starts_with(&format!(
                    "CreateInstance Windows.Foundation.Double {value} "
                ))
            })
            .count()
    };
    let clear = format!("ClearProperty 0x5 {RADIUS_X_INDEX}");

    // Glass until told.
    send(Command::SetLook(true));
    assert_eq!((made(&shared, "8"), made(&shared, "12")), (1, 0));

    send(Command::SetMaterial(Material::Solid));
    assert_eq!(shared.count(&clear), 1);
    assert_eq!((made(&shared, "8"), made(&shared, "12")), (1, 1));
    let calls = shared.calls();
    let cleared = calls.iter().position(|c| *c == clear).expect("cleared");
    let solid = calls
        .iter()
        .position(|c| c.starts_with("CreateInstance Windows.Foundation.Double 12 "))
        .expect("solid made");
    assert!(cleared < solid, "the glass value goes before the solid one");

    // The same material again changes nothing; back to glass swaps again.
    send(Command::SetMaterial(Material::Solid));
    assert_eq!(made(&shared, "12"), 1);
    send(Command::SetMaterial(Material::Glass));
    assert_eq!(shared.count(&clear), 2);
    assert_eq!(made(&shared, "8"), 2);

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}

static PAIR_THEN_ONE: [Rule; 3] = [
    Rule {
        material: Some(Material::Glass),
        ..look("RadiusX", Value::Length(8.0))
    },
    Rule {
        material: Some(Material::Solid),
        ..look("RadiusX", Value::Length(12.0))
    },
    look("RadiusX", Value::Length(16.0)),
];

#[test]
fn a_later_rule_for_both_materials_replaces_both_of_a_pair() {
    let _serial = serial();
    crate::host::tests::pretend_with(sepiida_core::shell::hosts::Host::Explorer, &PAIR_THEN_ONE);
    let (shared, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&shared);
    add_taskbar(&callback);
    let made = |shared: &Shared, value: &str| {
        shared
            .calls()
            .iter()
            .filter(|c| {
                c.starts_with(&format!(
                    "CreateInstance Windows.Foundation.Double {value} "
                ))
            })
            .count()
    };
    let clear = format!("ClearProperty 0x5 {RADIUS_X_INDEX}");

    send(Command::SetLook(true));
    send(Command::SetMaterial(Material::Solid));
    send(Command::SetMaterial(Material::Glass));
    assert_eq!(
        (made(&shared, "8"), made(&shared, "12"), made(&shared, "16")),
        (0, 0, 1)
    );
    assert_eq!(
        shared.count(&clear),
        0,
        "the rule for both stays across switches"
    );
    send(Command::SetLook(false));
    assert_eq!(shared.count(&clear), 1);

    send(Command::Detach);
    wait_until("Unadvise", || shared.has("Unadvise"));
}
