//! What an attachment has been told so far: colours, tint opacities and
//! whether the look is on. Commands arrive on the XAML threads that exist
//! when they are sent; a thread created later (a File Explorer window
//! opened after the last command) starts from this, so it is styled like
//! the rest without sepiida.exe sending everything again.

use super::{protocol::Command, rules::Role, tint::Opacity};
use crate::Color;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Carried {
    /// At most one per role, the latest.
    colors: Vec<(Role, Color)>,
    /// At most one per role; `None` is Windows' own opacity.
    opacities: Vec<(Role, Option<Opacity>)>,
    look: bool,
}

impl Carried {
    /// Takes in a command. `Restore` forgets everything, since the
    /// original values are back; `Detach` ends the attachment, so it
    /// changes nothing here.
    pub fn record(&mut self, command: Command) {
        match command {
            Command::SetColor(role, color) => set(&mut self.colors, role, color),
            Command::SetTintOpacity(role, opacity) => set(&mut self.opacities, role, opacity),
            Command::SetLook(on) => self.look = on,
            Command::Restore => *self = Self::default(),
            // Process-wide in the DLL: a new thread already sees it.
            Command::SetGdiAlpha(_) | Command::Detach => {}
        }
    }

    pub fn colors(&self) -> &[(Role, Color)] {
        &self.colors
    }

    pub fn opacities(&self) -> &[(Role, Option<Opacity>)] {
        &self.opacities
    }

    pub fn look(&self) -> bool {
        self.look
    }
}

fn set<T>(slots: &mut Vec<(Role, T)>, role: Role, value: T) {
    match slots.iter_mut().find(|(r, _)| *r == role) {
        Some(slot) => slot.1 = value,
        None => slots.push((role, value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Color = Color::new(0x33, 0x66, 0x99);
    const RED: Color = Color::new(0xff, 0, 0);

    #[test]
    fn the_latest_value_per_role_is_kept() {
        let mut carried = Carried::default();
        carried.record(Command::SetColor(Role::Surface, BLUE));
        carried.record(Command::SetColor(Role::Accent, RED));
        carried.record(Command::SetColor(Role::Surface, RED));
        assert_eq!(
            carried.colors(),
            [(Role::Surface, RED), (Role::Accent, RED)]
        );
        carried.record(Command::SetTintOpacity(Role::Surface, None));
        assert_eq!(carried.opacities(), [(Role::Surface, None)]);
    }

    #[test]
    fn the_look_follows_the_last_command() {
        let mut carried = Carried::default();
        assert!(!carried.look());
        carried.record(Command::SetLook(true));
        assert!(carried.look());
        carried.record(Command::SetLook(false));
        assert!(!carried.look());
    }

    #[test]
    fn restore_forgets_everything_and_detach_nothing() {
        let mut carried = Carried::default();
        carried.record(Command::SetColor(Role::Surface, BLUE));
        carried.record(Command::SetLook(true));
        let before = carried.clone();
        carried.record(Command::Detach);
        assert_eq!(carried, before);
        carried.record(Command::Restore);
        assert_eq!(carried, Carried::default());
    }
}
