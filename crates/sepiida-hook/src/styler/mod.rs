//! Paints the elements the rules select through XAML Diagnostics, and puts
//! the original values back: the targets (`targets`), the shared per-role
//! brushes of solid paint (`solid`), the tinted acrylic brushes (`tint`),
//! the look's layout values (`layout`, measured ones in `measured`) and
//! its own acrylic and transparent brushes (`acrylic`, `transparent`) and
//! XAML objects from theme lines (`markup`), and blur brushes (`blur`).

mod acrylic;
mod blur;
mod layout;
mod markup;
mod measured;
mod solid;
mod targets;
mod tint;
mod transparent;

pub use targets::Styler;
