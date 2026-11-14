//! What an attach asks of the DLL: the mode, and the colours to start
//! with.
//!
//! A sandboxed host (Start, quick settings) runs our DLL's tree callbacks
//! only once it is shown, which can be long after the attach; its control
//! window, the only way to send colours, does not exist before then. So the
//! colours travel with the attach, in the initialisation string the DLL
//! reads back with `GetInitializationData`, and the DLL applies them as
//! soon as targets appear.
//!
//! The string is `<tag>(;<role id>=<rrggbb>)*`, e.g.
//! `sepiida:style;1=336699`. sepiida.exe hands the DLL's attach export
//! fixed-size numbers, never a string, and the export writes the string
//! itself; the DLL parses it strictly and takes no colour from anything
//! malformed.

use super::{hosts::AttachMode, rules::Role};
use crate::Color;

/// The tag of style mode with colours. Style mode without colours passes
/// no string, as before colours could travel.
const STYLE_TAG: &str = "sepiida:style";
/// Longest initialisation string accepted; the longest valid one is well
/// under it.
pub const MAX_INIT_DATA: usize = 96;
/// The export's value for "no colour for this role".
const NO_COLOR: u32 = 0;
/// Marks an export value that carries a colour (so black is not "none").
const HAS_COLOR: u32 = 0x0100_0000;

/// One colour argument of the attach export per role, in `Role::ALL` order.
pub type ExportColors = [u32; Role::ALL.len()];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttachRequest {
    pub mode: AttachMode,
    /// At most one per role, in role order.
    colors: Vec<(Role, Color)>,
}

impl AttachRequest {
    /// A request; for one role given twice, the last colour wins. Dump
    /// mode styles nothing, so it carries no colours.
    pub fn new(mode: AttachMode, colors: &[(Role, Color)]) -> Self {
        let mut kept: Vec<(Role, Color)> = Vec::new();
        if mode != AttachMode::Dump {
            for &(role, color) in colors {
                match kept.iter_mut().find(|(r, _)| *r == role) {
                    Some(slot) => slot.1 = color,
                    None => kept.push((role, color)),
                }
            }
            kept.sort_by_key(|(role, _)| *role);
        }
        Self { mode, colors: kept }
    }

    pub fn colors(&self) -> &[(Role, Color)] {
        &self.colors
    }

    /// The initialisation string for this request; `None` passes no data
    /// (plain style mode).
    pub fn init_data(&self) -> Option<String> {
        let tag = match self.mode {
            AttachMode::Style if self.colors.is_empty() => return None,
            AttachMode::Style => STYLE_TAG,
            other => other.init_data()?,
        };
        let mut data = tag.to_owned();
        for (role, c) in &self.colors {
            data.push_str(&format!(";{}={:02x}{:02x}{:02x}", role.id(), c.r, c.g, c.b));
        }
        Some(data)
    }

    /// The request an initialisation string carries. An unknown tag means
    /// plain style mode; a malformed or repeated colour drops all colours.
    pub fn from_init_data(data: &str) -> Self {
        if data.len() > MAX_INIT_DATA {
            return Self::default();
        }
        let mut parts = data.split(';');
        let tag = parts.next().unwrap_or_default();
        let mode = AttachMode::from_init_data(tag);
        let known_tag = tag == STYLE_TAG || mode != AttachMode::Style;
        if !known_tag || mode == AttachMode::Dump {
            return Self::new(mode, &[]);
        }
        let mut colors = Vec::new();
        for part in parts {
            match parse_color(part) {
                Some((role, _)) if colors.iter().any(|(r, _)| *r == role) => {
                    return Self::new(mode, &[]);
                }
                Some(entry) => colors.push(entry),
                None => return Self::new(mode, &[]),
            }
        }
        Self::new(mode, &colors)
    }

    /// The colour arguments of the DLL's attach export.
    pub fn export_colors(&self) -> ExportColors {
        let mut values = [NO_COLOR; Role::ALL.len()];
        for (slot, role) in values.iter_mut().zip(Role::ALL) {
            if let Some((_, c)) = self.colors.iter().find(|(r, _)| *r == role) {
                *slot = HAS_COLOR | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b);
            }
        }
        values
    }

    /// The request the export's arguments describe, or `None` if a colour
    /// argument is malformed (stray high bits).
    pub fn from_export(mode: AttachMode, values: ExportColors) -> Option<Self> {
        let mut colors = Vec::new();
        for (value, role) in values.into_iter().zip(Role::ALL) {
            match value {
                NO_COLOR => {}
                v if v & 0xff00_0000 == HAS_COLOR => {
                    let byte = |shift: u32| ((v >> shift) & 0xff) as u8;
                    colors.push((role, Color::new(byte(16), byte(8), byte(0))));
                }
                _ => return None,
            }
        }
        Some(Self::new(mode, &colors))
    }
}

/// `<role id>=<rrggbb>`, exactly.
fn parse_color(part: &str) -> Option<(Role, Color)> {
    let (id, hex) = part.split_once('=')?;
    if id.len() != 1 || hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let role = Role::from_id(id.parse().ok()?)?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((role, Color::new(byte(0)?, byte(2)?, byte(4)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Color = Color::new(0x33, 0x66, 0x99);
    const BLACK: Color = Color::new(0, 0, 0);

    #[test]
    fn requests_round_trip_through_init_data_and_the_export() {
        for request in [
            AttachRequest::new(AttachMode::Style, &[]),
            AttachRequest::new(AttachMode::Style, &[(Role::Surface, BLUE)]),
            AttachRequest::new(
                AttachMode::StyleAlongside,
                &[(Role::Accent, BLACK), (Role::Surface, BLUE)],
            ),
            AttachRequest::new(AttachMode::StyleAlongside, &[]),
            AttachRequest::new(AttachMode::Dump, &[]),
        ] {
            let data = request.init_data().unwrap_or_default();
            assert_eq!(AttachRequest::from_init_data(&data), request, "{data}");
            assert_eq!(
                AttachRequest::from_export(request.mode, request.export_colors()),
                Some(request.clone())
            );
        }
    }

    #[test]
    fn the_string_is_tag_then_role_colours() {
        let request = AttachRequest::new(
            AttachMode::Style,
            &[(Role::Accent, BLACK), (Role::Surface, BLUE)],
        );
        assert_eq!(
            request.init_data().as_deref(),
            Some("sepiida:style;1=336699;2=000000")
        );
        assert_eq!(AttachRequest::new(AttachMode::Style, &[]).init_data(), None);
        assert!(
            AttachRequest::new(AttachMode::Style, &[(Role::Surface, BLUE)])
                .init_data()
                .is_some_and(|d| d.len() <= MAX_INIT_DATA)
        );
    }

    #[test]
    fn dump_mode_carries_no_colours_and_the_last_colour_wins() {
        let dump = AttachRequest::new(AttachMode::Dump, &[(Role::Surface, BLUE)]);
        assert!(dump.colors().is_empty());
        assert_eq!(dump.init_data().as_deref(), Some("sepiida:dump"));
        let twice = AttachRequest::new(
            AttachMode::Style,
            &[(Role::Surface, BLACK), (Role::Surface, BLUE)],
        );
        assert_eq!(twice.colors(), &[(Role::Surface, BLUE)]);
    }

    #[test]
    fn malformed_colours_are_all_dropped() {
        for data in [
            "sepiida:style;1=33669",
            "sepiida:style;1=3366999",
            "sepiida:style;1=33669g",
            "sepiida:style;3=336699",
            "sepiida:style;0=336699",
            "sepiida:style;1=336699;1=000000",
            "sepiida:style;1=336699;",
            "sepiida:style;11=336699",
            "sepiida:style;1=+36699",
            "sepiida:style;1=336699;2",
        ] {
            let request = AttachRequest::from_init_data(data);
            assert_eq!(request.mode, AttachMode::Style, "{data}");
            assert!(request.colors().is_empty(), "{data}");
        }
        let alongside = AttachRequest::from_init_data("sepiida:style-alongside;x");
        assert_eq!(alongside.mode, AttachMode::StyleAlongside);
        assert!(alongside.colors().is_empty());
    }

    #[test]
    fn unknown_tags_and_long_strings_mean_plain_style() {
        for data in [
            "other;1=336699",
            "SEPIIDA:STYLE;1=336699",
            "sepiida:dump;1=336699",
            &format!("sepiida:style;1=336699{}", " ".repeat(MAX_INIT_DATA)),
        ] {
            let request = AttachRequest::from_init_data(data);
            assert!(request.colors().is_empty(), "{data}");
        }
        assert_eq!(
            AttachRequest::from_init_data("sepiida:dump;1=336699").mode,
            AttachMode::Dump
        );
    }

    #[test]
    fn export_values_need_the_colour_marker() {
        assert_eq!(
            AttachRequest::from_export(AttachMode::Style, [0x0133_6699, 0])
                .map(|r| r.colors().to_vec()),
            Some(vec![(Role::Surface, BLUE)])
        );
        // Black is a colour, not "none".
        assert_eq!(
            AttachRequest::from_export(AttachMode::Style, [0x0100_0000, 0])
                .map(|r| r.colors().to_vec()),
            Some(vec![(Role::Surface, BLACK)])
        );
        assert_eq!(
            AttachRequest::from_export(AttachMode::Style, [0x0233_6699, 0]),
            None
        );
        assert_eq!(
            AttachRequest::from_export(AttachMode::Style, [0x0033_6699, 0]),
            None
        );
    }
}
