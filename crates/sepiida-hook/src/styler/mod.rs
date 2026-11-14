//! Paints the elements the rules select through XAML Diagnostics, and puts
//! the original values back: the targets (`targets`) and the shared
//! per-role brushes of solid paint (`solid`).

mod solid;
mod targets;

pub use targets::Styler;
