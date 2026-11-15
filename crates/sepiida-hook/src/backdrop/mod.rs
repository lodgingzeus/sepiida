//! File Explorer's glass backdrop: the DWM calls on each XAML thread's
//! folder windows (`glass`), the hooks that keep Explorer's own DWM
//! calls from undoing it (`hooks`), and those that keep its GDI-drawn
//! text readable on it (`gdi`).

mod gdi;
mod glass;
mod hooks;

pub use glass::{WindowBackdrop, flush};
