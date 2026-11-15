//! GDI drawing over File Explorer's glass. DWM reads the window's alpha
//! channel once the frame covers the whole window, and GDI leaves alpha at
//! zero, so the navigation pane, file list and status bar (Win32 and
//! DirectUI, not XAML) fade into the glass. The hook DLL redraws their text
//! with alpha and turns their background fills transparent; this is the
//! arithmetic it uses.
//!
//! Ported from the Windhawk "Windows 11 File Explorer Styler" mod
//! (GPL-3.0), whose rendering is based on the "Translucent Windows" mod.

pub mod fill;
pub mod parts;
pub mod text;

pub use fill::{DARK_BACKGROUND_PARTS, DIVIDER_ALPHA, divider_rect, is_window_background};
pub use text::{
    Rect, Run, alpha_table, colorize_mask, draw_text_area, extent_with_advances, text_rect,
};

/// A Windows `COLORREF`: `0x00BBGGRR`.
pub type ColorRef = u32;

/// The red, green and blue bytes of a `COLORREF`.
pub const fn rgb(color: ColorRef) -> (u8, u8, u8) {
    (color as u8, (color >> 8) as u8, (color >> 16) as u8)
}
