//! File Explorer's window backdrop: the glass behind the whole window, a
//! DWM accent blur tinted with the palette's surface colour. It is part of
//! the look, so it follows `SetLook`; the DLL makes the Windows calls, this
//! decides when and with which tint.
//!
//! Taken from the Windhawk "Windows 11 File Explorer Styler" mod (GPL-3.0):
//! its WindowGlass theme over the entire window, with the "Blur
//! (AccentBlurBehind)" effect.

use super::{framework::Framework, hosts::Host};
use crate::Color;

/// Window class of a File Explorer folder window.
pub const FOLDER_WINDOW_CLASS: &str = "CabinetWClass";
/// Child window class that marks the legacy frame (Control Panel): it has
/// no XAML, and its content is unreadable over the glass.
pub const LEGACY_FRAME_CHILD_CLASS: &str = "WorkerW";

/// Tint alpha of the blur, as in the mod (`0x3A232323`).
const TINT_ALPHA: u8 = 0x3a;
/// Tint used before a surface colour has arrived: the mod's.
const DEFAULT_TINT: Color = Color::new(0x23, 0x23, 0x23);

/// Whether the backdrop applies to `framework`'s XAML in `host`: File
/// Explorer's WinUI 3, not the taskbar beside it in the same explorer.exe.
pub fn applies(host: Option<Host>, framework: Framework) -> bool {
    host == Some(Host::Explorer) && framework == Framework::WinUi
}

/// Whether a top-level window of class `class` gets the backdrop.
pub fn is_glass_window(class: &str, has_legacy_frame_child: bool) -> bool {
    class.eq_ignore_ascii_case(FOLDER_WINDOW_CLASS) && !has_legacy_frame_child
}

/// The accent policy's `GradientColor` (`AABBGGRR`) for `tint`. The alpha
/// is never zero: an accent blur with a transparent tint draws nothing.
pub fn gradient_color(tint: Color) -> u32 {
    u32::from_le_bytes([tint.r, tint.g, tint.b, TINT_ALPHA])
}

/// What the DLL has to do to a thread's folder windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Put the glass on (again), with this tint.
    Apply(Color),
    /// Take the glass off and hand the window back to Windows.
    Remove,
}

/// One XAML thread's backdrop: whether the look is on, the tint, and
/// whether the glass is on its windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Backdrop {
    look: bool,
    tint: Option<Color>,
    applied: bool,
}

impl Backdrop {
    /// The look was turned on or off.
    pub fn set_look(&mut self, on: bool) -> Option<Action> {
        self.look = on;
        if on {
            Some(self.apply())
        } else {
            self.remove()
        }
    }

    /// The surface colour changed. Only a backdrop that is on is re-tinted.
    pub fn set_tint(&mut self, tint: Color) -> Option<Action> {
        if self.tint == Some(tint) {
            return None;
        }
        self.tint = Some(tint);
        self.look.then(|| self.apply())
    }

    /// Applies again while the look is on: Explorer may reset its backdrop
    /// (a new window not yet found, a theme change), and nothing tells us.
    pub fn refresh(&mut self) -> Option<Action> {
        self.look.then(|| self.apply())
    }

    /// Everything goes back to Windows (restore, detach, a new attach),
    /// and the tint is forgotten, as the styler forgets its colours.
    pub fn restore(&mut self) -> Option<Action> {
        let action = self.remove();
        *self = Self::default();
        action
    }

    pub fn is_applied(&self) -> bool {
        self.applied
    }

    fn apply(&mut self) -> Action {
        self.applied = true;
        Action::Apply(self.tint.unwrap_or(DEFAULT_TINT))
    }

    fn remove(&mut self) -> Option<Action> {
        std::mem::take(&mut self.applied).then_some(Action::Remove)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Color = Color::new(0x33, 0x66, 0x99);
    const RED: Color = Color::new(0xff, 0, 0);

    #[test]
    fn only_file_explorer_winui_gets_the_backdrop() {
        assert!(applies(Some(Host::Explorer), Framework::WinUi));
        assert!(!applies(Some(Host::Explorer), Framework::System));
        assert!(!applies(Some(Host::StartMenu), Framework::WinUi));
        assert!(!applies(None, Framework::WinUi));
    }

    #[test]
    fn folder_windows_without_the_legacy_frame_are_glass() {
        assert!(is_glass_window("CabinetWClass", false));
        assert!(is_glass_window("cabinetwclass", false));
        assert!(!is_glass_window("CabinetWClass", true));
        assert!(!is_glass_window("Shell_TrayWnd", false));
        assert!(!is_glass_window("", false));
    }

    #[test]
    fn the_gradient_is_abgr_with_the_mods_alpha() {
        assert_eq!(gradient_color(DEFAULT_TINT), 0x3a23_2323);
        assert_eq!(gradient_color(BLUE), 0x3a99_6633);
        assert_eq!(gradient_color(RED), 0x3a00_00ff);
    }

    #[test]
    fn look_on_applies_with_the_default_tint_until_a_colour_arrives() {
        let mut backdrop = Backdrop::default();
        assert_eq!(backdrop.set_look(true), Some(Action::Apply(DEFAULT_TINT)));
        assert!(backdrop.is_applied());
        assert_eq!(backdrop.set_tint(BLUE), Some(Action::Apply(BLUE)));
    }

    #[test]
    fn a_colour_while_off_is_kept_for_later() {
        let mut backdrop = Backdrop::default();
        assert_eq!(backdrop.set_tint(BLUE), None);
        assert_eq!(backdrop.set_look(true), Some(Action::Apply(BLUE)));
    }

    #[test]
    fn the_same_colour_twice_does_nothing() {
        let mut backdrop = Backdrop::default();
        backdrop.set_look(true);
        assert_eq!(backdrop.set_tint(BLUE), Some(Action::Apply(BLUE)));
        assert_eq!(backdrop.set_tint(BLUE), None);
    }

    #[test]
    fn look_off_removes_only_what_was_applied() {
        let mut backdrop = Backdrop::default();
        assert_eq!(backdrop.set_look(false), None);
        backdrop.set_look(true);
        assert_eq!(backdrop.set_look(false), Some(Action::Remove));
        assert_eq!(backdrop.set_look(false), None);
        assert!(!backdrop.is_applied());
    }

    #[test]
    fn refresh_applies_only_while_the_look_is_on() {
        let mut backdrop = Backdrop::default();
        assert_eq!(backdrop.refresh(), None);
        backdrop.set_tint(RED);
        backdrop.set_look(true);
        assert_eq!(backdrop.refresh(), Some(Action::Apply(RED)));
        backdrop.set_look(false);
        assert_eq!(backdrop.refresh(), None);
    }

    #[test]
    fn restore_removes_and_turns_the_look_off() {
        let mut backdrop = Backdrop::default();
        backdrop.set_tint(BLUE);
        backdrop.set_look(true);
        assert_eq!(backdrop.restore(), Some(Action::Remove));
        assert_eq!(backdrop.restore(), None);
        assert_eq!(backdrop.refresh(), None);
        assert_eq!(backdrop.set_look(true), Some(Action::Apply(DEFAULT_TINT)));
    }
}
