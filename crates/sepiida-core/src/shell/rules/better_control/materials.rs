//! Ours, after BetterControl11 (a later line wins): the materials. The
//! theme shapes the panels but leaves their backgrounds to Windows; here
//! they get our panel glass (`rules::glass`, as on Start and File
//! Explorer) and our rim, and notification cards stay opaque in the
//! raised surface colour so their text stays readable.

use crate::shell::framework::Framework;
use crate::shell::rules::{
    Role,
    edge::PANEL_EDGE,
    glass::PANEL_GLASS,
    windhawk::{Theme, Var},
};

const VARS: &[(&str, Var)] = &[
    ("Glass", PANEL_GLASS),
    ("Edge", PANEL_EDGE),
    ("Card", Var::Solid(Role::SurfaceRaised)),
];

pub const MATERIALS: Theme = Theme {
    framework: Framework::System,
    material: None,
    targets: &[
        (
            "Grid#NotificationCenterGrid, Grid#CalendarCenterGrid, Grid#ControlCenterRegion, Grid#MediaTransportControlsRegion",
            &["Background:=$Glass", "BorderBrush:=$Edge"],
        ),
        ("Grid > Border#ItemOpaquePlating", &["Background:=$Card"]),
    ],
    vars: VARS,
};
