//! Paints the elements the rules select through XAML Diagnostics, and puts
//! the original values back: the targets (`targets`), the shared per-role
//! brushes of solid paint (`solid`), the tinted acrylic brushes (`tint`),
//! the look's layout values (`layout`, measured ones in `measured`) and
//! its own acrylic and transparent brushes (`acrylic`, `transparent`) and
//! XAML objects from theme lines (`markup`), blur brushes (`blur`) and
//! attached properties (`attached`); selectors' property conditions
//! (`condition`).

mod acrylic;
mod attached;
mod blur;
mod condition;
mod layout;
mod markup;
mod measured;
mod solid;
mod targets;
mod tint;
mod transparent;

pub use targets::Styler;
