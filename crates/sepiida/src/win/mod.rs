//! Everything that talks to Windows. Decisions live in `sepiida-core`; this
//! module only reads and writes.

pub mod desktop;
pub mod desktop_events;
pub mod registry;
pub mod single_instance;
pub mod theme;
pub mod wallpaper_engine;
