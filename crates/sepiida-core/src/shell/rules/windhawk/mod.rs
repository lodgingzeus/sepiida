//! Rules written in the syntax of Windhawk's Windows 11 styler mods
//! (windhawk-mods, GPL-3.0), so their themes can be pasted as they are:
//! target selectors (`selector`), style lines (`style`) and whole themes
//! with our variables in place of theirs (`theme`). Parsed once per
//! process into ordinary rules; the strings are leaked for that process's
//! life (`leak`), so rules stay `&'static` like the hand-written ones.

mod leak;
mod selector;
mod style;
mod theme;

#[cfg(test)]
mod tests;

use leak::leak;
pub use selector::{SelectorError, Target, parse_targets};
pub use style::{Style, StyleError, StyleValue, parse_style};
pub use theme::{Built, Skipped, Theme, Var, build, build_all, build_on};
