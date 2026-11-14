//! Strings made while parsing a theme (full type names) live as long as
//! the process: rule sets are built once per process and never freed, so
//! the leak is bounded by the size of the shipped themes.

pub fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}
