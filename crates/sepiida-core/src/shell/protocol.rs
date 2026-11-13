//! What sepiida.exe and the hook DLL inside Explorer agree on: names, the
//! class ID, and how commands are encoded in window messages.
//!
//! Commands travel as posted messages to a message-only window the DLL
//! creates on Explorer's XAML thread, which is also where the XAML elements
//! have to be changed. Each command fits in one message, so the DLL never
//! parses a buffer from another process; every `(message, wParam)` pair
//! either decodes to a command or is ignored.

use crate::Color;

/// Class ID of the DLL's `IObjectWithSite` object, passed to
/// `InitializeXamlDiagnosticsEx`.
pub const TAP_CLSID: u128 = 0xb62a4d11_0403_4822_8c76_7ec4d7181ff1;

/// Window class of the DLL's message-only command window.
pub const CONTROL_WINDOW_CLASS: &str = "sepiida.hook.control";

/// System XAML names each XAML thread's diagnostics port
/// `VisualDiagConnection<N>`, counting up as threads are created over the
/// life of the process, so the number is not known in advance.
pub const CONNECTION_PREFIX: &str = "VisualDiagConnection";
/// Connection numbers tried before giving up. Explorer creates few XAML
/// threads; the taskbar's is among the first.
pub const MAX_CONNECTION: u32 = 256;

/// Every connection name to try, in order.
pub fn connection_names() -> impl Iterator<Item = (u32, String)> {
    (1..=MAX_CONNECTION).map(|n| (n, format!("{CONNECTION_PREFIX}{n}")))
}

/// Named event the DLL sets once it is watching the visual tree. Per
/// session (`Local\`) and per target process.
pub fn ready_event_name(pid: u32) -> String {
    format!("Local\\sepiida-hook-ready-{pid}")
}

/// `WM_APP`: the start of the message range private to a window class.
const WM_APP: u32 = 0x8000;
const MSG_SET_COLOR: u32 = WM_APP + 0x5e1;
const MSG_RESTORE: u32 = WM_APP + 0x5e2;
const MSG_DETACH: u32 = WM_APP + 0x5e3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Paint the taskbar background in this colour.
    SetColor(Color),
    /// Put back the original brushes and keep watching.
    Restore,
    /// Put back the original brushes and stop watching. The DLL stays
    /// loaded (XAML never frees it) but does nothing until attached again.
    Detach,
}

impl Command {
    /// The message and `wParam` that carry this command.
    pub fn encode(self) -> (u32, usize) {
        match self {
            Self::SetColor(c) => (
                MSG_SET_COLOR,
                (usize::from(c.r) << 16) | (usize::from(c.g) << 8) | usize::from(c.b),
            ),
            Self::Restore => (MSG_RESTORE, 0),
            Self::Detach => (MSG_DETACH, 0),
        }
    }

    /// The command a received message carries, or `None` for any other
    /// message. Unused `wParam` bits must be zero.
    pub fn decode(message: u32, wparam: usize) -> Option<Self> {
        match (message, wparam) {
            (MSG_SET_COLOR, w) if w <= 0x00ff_ffff => {
                let channel = |shift: u32| ((w >> shift) & 0xff) as u8;
                Some(Self::SetColor(Color::new(
                    channel(16),
                    channel(8),
                    channel(0),
                )))
            }
            (MSG_RESTORE, 0) => Some(Self::Restore),
            (MSG_DETACH, 0) => Some(Self::Detach),
            _ => None,
        }
    }
}

/// An opaque colour in the `#AARRGGBB` form XAML parses.
pub fn xaml_color(c: Color) -> String {
    format!("#FF{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for command in [
            Command::SetColor(Color::new(0x12, 0x34, 0x56)),
            Command::SetColor(Color::new(0, 0, 0)),
            Command::SetColor(Color::new(255, 255, 255)),
            Command::Restore,
            Command::Detach,
        ] {
            let (message, wparam) = command.encode();
            assert_eq!(Command::decode(message, wparam), Some(command));
        }
    }

    #[test]
    fn colour_is_packed_as_rgb() {
        let (_, wparam) = Command::SetColor(Color::new(0x12, 0x34, 0x56)).encode();
        assert_eq!(wparam, 0x0012_3456);
    }

    #[test]
    fn stray_bits_and_other_messages_are_ignored() {
        let (set, _) = Command::SetColor(Color::new(0, 0, 0)).encode();
        let (restore, _) = Command::Restore.encode();
        let (detach, _) = Command::Detach.encode();
        assert_eq!(Command::decode(set, 0x0100_0000), None);
        assert_eq!(Command::decode(restore, 1), None);
        assert_eq!(Command::decode(detach, 1), None);
        assert_eq!(Command::decode(WM_APP, 0), None);
        assert_eq!(Command::decode(0x0010, 0), None); // WM_CLOSE
    }

    #[test]
    fn xaml_colour_is_opaque_argb() {
        assert_eq!(xaml_color(Color::new(0x0a, 0xbc, 0xff)), "#FF0ABCFF");
    }

    #[test]
    fn connection_names_count_from_one() {
        let names: Vec<_> = connection_names().collect();
        assert_eq!(names.len(), MAX_CONNECTION as usize);
        assert_eq!(names[0], (1, "VisualDiagConnection1".to_owned()));
        assert_eq!(names.last().map(|(n, _)| *n), Some(MAX_CONNECTION));
    }

    #[test]
    fn ready_event_is_per_session_and_process() {
        assert_eq!(ready_event_name(4242), "Local\\sepiida-hook-ready-4242");
    }
}
