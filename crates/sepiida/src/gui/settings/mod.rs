//! The settings window ("Wallpaper & style"). `model` is its plain-data
//! half (option orders, the start-with-Windows command), tested on Linux;
//! `window` drives the Slint window and `preview` reads the wallpaper.

pub mod model;
#[cfg(windows)]
mod preview;
#[cfg(windows)]
mod window;

#[cfg(windows)]
pub use window::{open, reload_preview};
