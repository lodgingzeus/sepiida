//! GDI drawing over File Explorer's glass: import-table hooks on the
//! GDI, user32 and uxtheme calls that paint the navigation pane, file
//! list and status bar, so their text gets alpha and their background
//! fills become transparent (the arithmetic is
//! `sepiida_core::shell::gdi_glass`).
//!
//! On with the glass and off with it, process-wide. A panic in any hook
//! turns them all into pass-throughs at once and they are removed at the
//! next chance (`sync`); `shell gdi off` does the same by hand. Only
//! panics are caught: a fault in GDI itself still ends Explorer.
//!
//! Ported from the Windhawk "Windows 11 File Explorer Styler" mod
//! (GPL-3.0), based on the "Translucent Windows" mod. Two of its hooks are
//! on functions called inside their own DLL, which import tables do not
//! reach: uxtheme's `DrawTextWithGlow` is hooked where a module imports it
//! by ordinal, and composited `DrawThemeTextEx` text is left to uxtheme;
//! dui70's `Element::PaintBorder` is not hooked, so some DirectUI borders
//! keep alpha 0.

mod draw_text;
mod fill;
mod install;
mod mask;
mod paint;
mod parts;
mod state;
mod text;

pub use install::{install, set_enabled, sync};
pub use state::set_thread_glass;

#[cfg(all(test, windows))]
mod tests;
