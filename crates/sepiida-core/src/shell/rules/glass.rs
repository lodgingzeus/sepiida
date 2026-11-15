//! Our panel glass, shared by Start and the flyouts so they match File
//! Explorer's window glass: a strong blur under a light tint of the
//! palette's surface colour (Explorer's tint alpha is `0x3A`, about
//! 23 %); a solid fill of the colour for the solid material.

use crate::shell::rules::{Role, windhawk::Var};

/// The blur radius and tint opacity of `PANEL_GLASS`.
pub const PANEL_BLUR: f32 = 30.0;
pub const PANEL_TINT: f32 = 0.23;

pub const PANEL_GLASS: Var = Var::Material {
    glass: &Var::Blur {
        amount: PANEL_BLUR,
        role: Role::Surface,
        opacity: PANEL_TINT,
    },
    solid: &Var::Fill(Role::Surface),
};
