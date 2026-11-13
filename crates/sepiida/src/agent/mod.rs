//! The background agent (`sepiida run`, `sepiidaw.exe`): its main loop and
//! the watchers that wake it.

#[cfg(windows)]
mod engine_watch;
#[cfg(windows)]
mod run;
#[cfg(any(windows, test))]
mod watch;

#[cfg(windows)]
pub use run::run;
