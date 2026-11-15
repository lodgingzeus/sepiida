//! Windhawk style lines: `Property=Value`, made by XAML from the string,
//! and `Property:=<xaml>`, a XAML object (loaded by the DLL, `markup`). Not
//! supported yet, and reported
//! as such: `Property@State=Value` (visual states) and `Property=>Var`
//! (captures). Attached properties (`Grid.Column`) are read like others.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub property: &'static str,
    pub value: StyleValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleValue {
    /// `Property=Value`.
    Text(&'static str),
    /// `Property:=Value`: XAML markup or a `$Variable`.
    Xaml(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleError {
    NoEquals,
    EmptyProperty,
    Unsupported(&'static str),
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEquals => f.write_str("no '=' in the style"),
            Self::EmptyProperty => f.write_str("empty property name"),
            Self::Unsupported(what) => write!(f, "{what} is not supported yet"),
        }
    }
}

pub fn parse_style(text: &'static str) -> Result<Style, StyleError> {
    let (name, value) = text.split_once('=').ok_or(StyleError::NoEquals)?;
    if value.starts_with('>') {
        return Err(StyleError::Unsupported("a capture (=>)"));
    }
    let (name, xaml) = match name.strip_suffix(':') {
        Some(name) => (name, true),
        None => (name, false),
    };
    if name.contains('@') {
        return Err(StyleError::Unsupported("@VisualState"));
    }
    let property = name.trim();
    if property.is_empty() {
        return Err(StyleError::EmptyProperty);
    }
    if property.contains('.') && !crate::shell::markup::is_attached(property) {
        return Err(StyleError::Unsupported(
            "a dotted name other than Owner.Property",
        ));
    }
    let value = value.trim();
    Ok(Style {
        property,
        value: if xaml {
            StyleValue::Xaml(value)
        } else {
            StyleValue::Text(value)
        },
    })
}
