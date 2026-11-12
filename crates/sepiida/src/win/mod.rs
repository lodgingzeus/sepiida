//! Everything that talks to Windows. Decisions live in `sepiida-core`; this
//! module only reads and writes.

pub mod desktop;
pub mod desktop_events;
pub mod message_box;
pub mod registry;
pub mod single_instance;
pub mod startup;
pub mod theme;
pub mod wallpaper_engine;
