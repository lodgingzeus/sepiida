//! Look rules: layout values (margins, corners, visibility) set on the
//! elements they select while the look is on, and put back when it is
//! turned off. What to write and how to restore comes from
//! `sepiida_core::shell::layout::plan_write`; this module only makes the
//! calls.
//!
//! The original is read when a value is written, not when the element is
//! found: turning the look off and on again reads it afresh. An acrylic
//! value waits for its role's colour, which also re-tints it later.
//!
//! `state` holds the targets and the switches (on/off, material,
//! colours); `write` writes a value and `restore` puts the original back;
//! `sizes` reads the sizes Windows may set again; `held` lowers the
//! luminosity of an acrylic an animation holds where our glass is due.

mod held;
mod restore;
mod sizes;
mod state;
mod write;

pub use state::Layouts;
