//! Platform-independent core of sepiida: the colour engine, settings and
//! state files, wallpaper sources, and the data side of shell styling.
//! Nothing here calls Windows APIs, so all of it is tested on Linux.

pub mod accent;
pub mod cache;
pub mod choice;
mod color;
pub mod config;
mod fsio;
mod palette;
pub mod pe;
pub mod schedule;
pub mod settings_look;
pub mod shell;
pub mod snapshot;
pub mod source;
pub mod thumbnail;
pub mod wallpaper_engine;

pub use color::{Color, InvalidColor};
pub use palette::{Candidates, Error, MAX_CANDIDATES, Palette, Style, Tones};
