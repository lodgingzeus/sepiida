//! Everything that talks to Windows. Decisions live in `sepiida-core`; this
//! module only reads and writes.

pub mod desktop;
pub mod desktop_events;
pub mod file_access;
pub mod message_box;
pub mod mode_watch;
pub mod modules;
pub mod process_info;
pub mod processes;
pub mod registry;
pub mod shell_hook;
pub mod single_instance;
pub mod startup;
pub mod theme;
pub mod wallpaper_engine;
