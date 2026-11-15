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
//! The string is `<tag>[@winui](;<role id>=<rrggbb>)*`, e.g.
//! `sepiida:style;1=336699` or `sepiida:dump@winui`. `@winui` marks an
//! attach to File Explorer's WinUI 3 XAML, so the DLL picks that
//! framework's rules and value types and keeps the attachment apart from
//! the taskbar's in the same process. sepiida.exe hands the DLL's attach export
//! one fixed-size number per role (a pointer and a count it checks),
//! never a string, and the export writes the string itself; the DLL parses it strictly and takes no colour from anything
//! malformed.

use super::{framework::Framework, hosts::AttachMode, rules::Role};
use crate::Color;

/// The tag of style mode with colours. Style mode without colours passes
/// no string, as before colours could travel.
const STYLE_TAG: &str = "sepiida:style";
/// Appended to the tag for WinUI 3 (`Framework::WinUi`).
const WINUI_SUFFIX: &str = "@winui";
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
    pub framework: Framework,
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
        Self {
            mode,
            framework: Framework::System,
            colors: kept,
        }
    }

    /// The same request for `framework`'s XAML.
    pub fn for_framework(mut self, framework: Framework) -> Self {
        self.framework = framework;
        self
    }

    pub fn colors(&self) -> &[(Role, Color)] {
        &self.colors
    }

    /// The initialisation string for this request; `None` passes no data
    /// (plain style mode).
    pub fn init_data(&self) -> Option<String> {
        let winui = self.framework == Framework::WinUi;
        let tag = match self.mode {
            AttachMode::Style if self.colors.is_empty() && !winui => return None,
            AttachMode::Style => STYLE_TAG,
            other => other.init_data()?,
        };
        let mut data = tag.to_owned();
        if winui {
            data.push_str(WINUI_SUFFIX);
        }
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
        let full_tag = parts.next().unwrap_or_default();
        let (tag, framework) = match full_tag.strip_suffix(WINUI_SUFFIX) {
            Some(tag) => (tag, Framework::WinUi),
            None => (full_tag, Framework::System),
        };
        let mode = AttachMode::from_init_data(tag);
        let known_tag = tag == STYLE_TAG || mode != AttachMode::Style;
        if !known_tag {
            return Self::default();
        }
        if mode == AttachMode::Dump {
            return Self::new(mode, &[]).for_framework(framework);
        }
        let mut colors = Vec::new();
        for part in parts {
            match parse_color(part) {
                Some((role, _)) if colors.iter().any(|(r, _)| *r == role) => {
                    return Self::new(mode, &[]).for_framework(framework);
                }
                Some(entry) => colors.push(entry),
                None => return Self::new(mode, &[]).for_framework(framework),
            }
        }
        Self::new(mode, &colors).for_framework(framework)
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
mod tests;
