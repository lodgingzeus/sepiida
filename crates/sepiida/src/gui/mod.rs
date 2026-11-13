//! The Slint user interface: the tray icon and the settings window. The
//! plain-data halves (`settings_model`, `tray_icon`) are tested on Linux.

#[cfg(windows)]
pub mod settings;
pub mod settings_model;
#[cfg(windows)]
pub mod tray;
pub mod tray_icon;
