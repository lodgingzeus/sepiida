//! The WindowGlass theme by Nathaniel4JC, from Windhawk's Windows 11
//! styler mods by m417z (https://github.com/ramensoftware/windhawk-mods,
//! GPL-3.0): start-menu-styler v1.8 (commit `cc94527`),
//! notification-center-styler v1.8 (`4197cf6`) and taskbar-styler v1.11.1
//! (`2cd9c60`). Target and style lines are copied unchanged, except lines
//! that name fonts we cannot ship or the author's own text; each file says
//! what it leaves out. Only the variables are ours: the glass blur takes
//! the palette's surface colour, panel edges are our own (`edge`), and the
//! theme's neutral veils and inner card borders stay as written (its `SystemAccentColor` resources follow the
//! palette already, since sepiida sets the accent colour).

mod edge;
mod flyout_controls;
mod flyout_panels;
mod start;
mod taskbar;

pub use flyout_controls::FLYOUT_CONTROLS;
pub use flyout_panels::{FLYOUT_ADDITIONS, FLYOUT_PANELS};
pub use start::START;
pub use taskbar::{TASKBAR, TASKBAR_ADDITIONS};
