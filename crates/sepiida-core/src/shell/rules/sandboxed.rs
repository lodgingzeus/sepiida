//! Hand-written rule builders for the sandboxed hosts (Start, ShellHost,
//! ShellExperienceHost): a tinted surface and an overlay that is only
//! logged. Most of their rules are Windhawk theme lines (`windhawk`).

use super::rule::{BORDER_TYPE, Effect, Name, Paint, Role, Rule};

/// A surface background: tinted acrylic.
pub(super) const fn tinted(
    label: &'static str,
    type_name: &'static str,
    name: &'static str,
) -> Rule {
    Rule {
        label,
        type_name,
        name: Name::Is(name),
        ancestor: None,
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
    }
}

/// A border that may lie over a surface; only logged.
pub(super) const fn overlay(label: &'static str, name: &'static str) -> Rule {
    Rule {
        effect: Effect::Color {
            paint: Paint::LogOnly,
            role: Role::Surface,
        },
        ..tinted(label, BORDER_TYPE, name)
    }
}
