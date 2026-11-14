//! Paints the elements the rules select through XAML Diagnostics, and puts
//! the original values back: the targets (`targets`), the shared per-role
//! brushes of solid paint (`solid`), the tinted acrylic brushes (`tint`),
//! the look's layout values (`layout`, measured ones in `measured`) and
//! its own acrylic brushes (`acrylic`).

mod acrylic;
mod layout;
mod measured;
mod solid;
mod targets;
mod tint;

pub use targets::Styler;
