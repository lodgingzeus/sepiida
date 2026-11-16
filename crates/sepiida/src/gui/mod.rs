//! The Slint user interface: the tray icon and the settings window. The
//! plain-data halves (`settings::model`, `tray_icon`) are tested on Linux.

pub mod settings;
#[cfg(windows)]
pub mod tray;
pub mod tray_icon;
