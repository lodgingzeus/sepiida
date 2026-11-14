//! Paints the elements the rules select through XAML Diagnostics, and puts
//! the original values back: the targets (`targets`), the shared per-role
//! brushes of solid paint (`solid`) and the tinted acrylic brushes
//! (`tint`).

mod solid;
mod targets;
mod tint;

pub use targets::Styler;
