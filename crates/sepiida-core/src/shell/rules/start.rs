//! The Start menu (StartMenuExperienceHost).

use super::{
    rule::{BORDER_TYPE, Rule},
    sandboxed::{overlay, tinted},
};

/// Build 26100: `Grid#RootGrid > Grid#RootContent > Border#AcrylicBorder`,
/// with overlays nearby.
pub const START_RULES: &[Rule] = &[
    tinted("Start background", BORDER_TYPE, "AcrylicBorder"),
    overlay("Start acrylic overlay", "AcrylicOverlay"),
    overlay("Start colour overlay", "BackgroundColorOverlay"),
    overlay("Start full-bleed fallback", "FallbackFullBleedBackground"),
];
