//! The WindowGlass theme by Nathaniel4JC, from Windhawk's Windows 11
//! styler mods by m417z (https://github.com/ramensoftware/windhawk-mods,
//! GPL-3.0), for the shell's popups, the taskbar and File Explorer:
//! notification-center-styler v1.8 (commit `4197cf6`), taskbar-styler v1.11.1
//! (`2cd9c60`) and file-explorer-styler v1.7.2 (`12b5a5c`). Target and style lines are copied unchanged, except lines
//! that name fonts we cannot ship or the author's own text; each file says
//! what it leaves out. Only the variables are ours: the glass blur takes
//! the palette's surface colour, panel edges are our own (`rules::edge`), and the
//! theme's neutral veils and inner card borders stay as written (its `SystemAccentColor` resources follow the
//! palette already, since sepiida sets the accent colour).

mod explorer;
mod popups;
mod taskbar;

pub use explorer::EXPLORER;
pub use popups::POPUPS;
pub use taskbar::{TASKBAR, TASKBAR_ADDITIONS};
