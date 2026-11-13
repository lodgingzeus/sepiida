//! What sepiida.exe and the hook DLL inside Explorer agree on: names, the
//! class ID, and how commands are encoded in window messages.
//!
//! Commands travel as posted messages to a message-only window the DLL
//! creates on Explorer's XAML thread, which is also where the XAML elements
//! have to be changed. Each command fits in one message, so the DLL never
//! parses a buffer from another process; every `(message, wParam)` pair
//! either decodes to a command or is ignored.

use super::rules::Role;
use crate::Color;

/// Class ID of the DLL's `IObjectWithSite` object, passed to
/// `InitializeXamlDiagnosticsEx`.
pub const TAP_CLSID: u128 = 0xb62a4d11_0403_4822_8c76_7ec4d7181ff1;

/// Window class of the DLL's message-only command window. Every version of
/// the DLL uses it, so a new attach can find and detach older copies.
pub const CONTROL_WINDOW_CLASS: &str = "sepiida.hook.control";

/// Version of the commands below. Bumped whenever their meaning changes;
/// sepiida.exe sends commands only to control windows of its own version,
/// except `Detach`, which every version understands.
pub const PROTOCOL_VERSION: u32 = 2;

/// Title of a control window speaking `PROTOCOL_VERSION`.
pub fn control_window_title() -> String {
    format!("{CONTROL_WINDOW_CLASS} v{PROTOCOL_VERSION}")
}

/// System XAML names each XAML thread's diagnostics port
/// `VisualDiagConnection<N>`, counting up as threads are created over the
/// life of the process, so the number is not known in advance.
pub const CONNECTION_PREFIX: &str = "VisualDiagConnection";
/// Connection numbers tried before giving up. Explorer creates few XAML
/// threads; the taskbar's is among the first.
pub const MAX_CONNECTION: u32 = 256;

/// Every connection name to try from number `from` on, in order. A `from`
/// of 0 counts as 1.
pub fn connection_names(from: u32) -> impl Iterator<Item = (u32, String)> {
    (from.max(1)..=MAX_CONNECTION).map(|n| (n, format!("{CONNECTION_PREFIX}{n}")))
}

/// DLL export that calls `InitializeXamlDiagnosticsEx` for a process:
/// `extern "system" fn(pid: u32, from: u32, mode: u32, connection: *mut u32)
/// -> HRESULT`, `mode` being an `AttachMode` id.
pub const INITIALIZE_EXPORT: &str = "SepiidaInitializeXamlDiagnostics";
/// DLL export used as a `WH_CALLWNDPROC` hook procedure on the taskbar's
/// thread, to start the in-process attach.
pub const HOOK_PROC_EXPORT: &str = "SepiidaCallWndProc";
/// Registered window message sent to the taskbar window to trigger the
/// in-process attach; `wParam` is the first connection number to try.
pub const ATTACH_MESSAGE: &str = "sepiida.hook.attach";

/// Named event the DLL sets once it is watching the visual tree. Per
/// session (`Local\`) and per target process.
pub fn ready_event_name(pid: u32) -> String {
    format!("Local\\sepiida-hook-ready-{pid}")
}

/// Named event the DLL sets when an in-process attach gives up (no XAML
/// diagnostics port answered, typically because Explorer's XAML is not up
/// yet), so sepiida.exe stops waiting at once.
pub fn failed_event_name(pid: u32) -> String {
    format!("Local\\sepiida-hook-failed-{pid}")
}

/// `WM_APP`: the start of the message range private to a window class.
const WM_APP: u32 = 0x8000;
// WM_APP + 0x5e1 was version 1's single taskbar colour; never reuse it.
// Restore and Detach keep their numbers so that every version understands
// them.
const MSG_RESTORE: u32 = WM_APP + 0x5e2;
const MSG_DETACH: u32 = WM_APP + 0x5e3;
const MSG_SET_COLOR: u32 = WM_APP + 0x5e4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Paint every target of this role in this colour.
    SetColor(Role, Color),
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
            Self::SetColor(role, c) => (
                MSG_SET_COLOR,
                (usize::from(role.id()) << 24)
                    | (usize::from(c.r) << 16)
                    | (usize::from(c.g) << 8)
                    | usize::from(c.b),
            ),
            Self::Restore => (MSG_RESTORE, 0),
            Self::Detach => (MSG_DETACH, 0),
        }
    }

    /// The command a received message carries, or `None` for any other
    /// message. Unused `wParam` bits must be zero and the role known.
    pub fn decode(message: u32, wparam: usize) -> Option<Self> {
        match (message, wparam) {
            (MSG_SET_COLOR, w) if w <= 0xffff_ffff => {
                let byte = |shift: u32| ((w >> shift) & 0xff) as u8;
                let role = Role::from_id(byte(24))?;
                Some(Self::SetColor(role, Color::new(byte(16), byte(8), byte(0))))
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
            Command::SetColor(Role::Surface, Color::new(0x12, 0x34, 0x56)),
            Command::SetColor(Role::Accent, Color::new(0, 0, 0)),
            Command::SetColor(Role::Surface, Color::new(255, 255, 255)),
            Command::Restore,
            Command::Detach,
        ] {
            let (message, wparam) = command.encode();
            assert_eq!(Command::decode(message, wparam), Some(command));
        }
    }

    #[test]
    fn colour_is_packed_as_role_then_rgb() {
        let (_, wparam) = Command::SetColor(Role::Surface, Color::new(0x12, 0x34, 0x56)).encode();
        assert_eq!(wparam, 0x0112_3456);
    }

    #[test]
    fn stray_bits_unknown_roles_and_other_messages_are_ignored() {
        let (set, _) = Command::SetColor(Role::Surface, Color::new(0, 0, 0)).encode();
        let (restore, _) = Command::Restore.encode();
        let (detach, _) = Command::Detach.encode();
        assert_eq!(Command::decode(set, 0x0012_3456), None); // role 0
        assert_eq!(Command::decode(set, 0x0312_3456), None); // role 3
        #[cfg(target_pointer_width = "64")]
        assert_eq!(Command::decode(set, 0x1_0112_3456), None);
        assert_eq!(Command::decode(restore, 1), None);
        assert_eq!(Command::decode(detach, 1), None);
        assert_eq!(Command::decode(WM_APP, 0), None);
        assert_eq!(Command::decode(0x0010, 0), None); // WM_CLOSE
        // Version 1's single-colour message.
        assert_eq!(Command::decode(WM_APP + 0x5e1, 0x0012_3456), None);
    }

    #[test]
    fn restore_and_detach_keep_their_version_1_numbers() {
        assert_eq!(Command::Restore.encode(), (WM_APP + 0x5e2, 0));
        assert_eq!(Command::Detach.encode(), (WM_APP + 0x5e3, 0));
    }

    #[test]
    fn the_control_window_title_carries_the_version() {
        assert_eq!(control_window_title(), "sepiida.hook.control v2");
    }

    #[test]
    fn xaml_colour_is_opaque_argb() {
        assert_eq!(xaml_color(Color::new(0x0a, 0xbc, 0xff)), "#FF0ABCFF");
    }

    #[test]
    fn connection_names_count_from_one() {
        let names: Vec<_> = connection_names(1).collect();
        assert_eq!(names.len(), MAX_CONNECTION as usize);
        assert_eq!(names[0], (1, "VisualDiagConnection1".to_owned()));
        assert_eq!(
            connection_names(0).next(),
            Some((1, "VisualDiagConnection1".to_owned()))
        );
        assert_eq!(connection_names(5).next().map(|(n, _)| n), Some(5));
        assert_eq!(connection_names(MAX_CONNECTION + 1).count(), 0);
        assert_eq!(names.last().map(|(n, _)| *n), Some(MAX_CONNECTION));
    }

    #[test]
    fn ready_event_is_per_session_and_process() {
        assert_eq!(ready_event_name(4242), "Local\\sepiida-hook-ready-4242");
        assert_eq!(failed_event_name(4242), "Local\\sepiida-hook-failed-4242");
    }
}
