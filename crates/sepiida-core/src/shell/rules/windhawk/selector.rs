//! Windhawk target selectors: `Type#Name > Type > * > Type#Name`, several
//! separated by commas. The last part is the element; the others are its
//! ancestors, ` > ` meaning "direct parent of" and `*` any number of
//! ancestors in between. A type without a namespace is a `Controls` type
//! of the theme's framework (`Windows.UI.Xaml.Controls` for the shell,
//! `Microsoft.UI.Xaml.Controls` for File Explorer), `Rectangle` a shape,
//! and `muxc:` stands for `Microsoft.UI.Xaml.Controls.`, as in the
//! Windhawk stylers.
//!
//! Not supported yet, and reported as such: `[n]` (child index),
//! `[Prop=Value]` (property condition), `@Group` (visual state) and
//! `:root`.

use std::fmt;

use super::leak;
use crate::shell::{
    framework::Framework,
    rules::{Name, Step},
};

/// One parsed selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub type_name: &'static str,
    pub name: Name,
    /// Ancestors, nearest first.
    pub chain: &'static [Step],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorError {
    Empty,
    EmptyType,
    EmptyName,
    TwoNames,
    /// `*` as the element, the leftmost part, or twice in a row.
    MisplacedWildcard,
    Unsupported(&'static str),
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty target"),
            Self::EmptyType => f.write_str("a part has no type"),
            Self::EmptyName => f.write_str("empty name after '#'"),
            Self::TwoNames => f.write_str("a part has two names"),
            Self::MisplacedWildcard => f.write_str("misplaced '*'"),
            Self::Unsupported(what) => write!(f, "{what} is not supported yet"),
        }
    }
}

enum Part {
    Element { type_name: &'static str, name: Name },
    Wildcard,
}

/// Every selector in `text`, split on the commas outside `[…]`; types
/// without a namespace belong to `framework`.
pub fn parse_targets(
    text: &'static str,
    framework: Framework,
) -> Result<Vec<Target>, SelectorError> {
    split_targets(text)
        .into_iter()
        .map(|target| parse_target(target, framework))
        .collect()
}

fn split_targets(text: &'static str) -> Vec<&'static str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, c) in text.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn parse_target(text: &'static str, framework: Framework) -> Result<Target, SelectorError> {
    if text.trim().is_empty() {
        return Err(SelectorError::Empty);
    }
    let parts: Vec<Part> = text
        .split(" > ")
        .map(|part| parse_part(part, framework))
        .collect::<Result<_, _>>()?;
    let Some((Part::Element { type_name, name }, ancestors)) = parts.split_last() else {
        return Err(SelectorError::MisplacedWildcard);
    };
    let mut chain = Vec::with_capacity(ancestors.len());
    for (i, part) in ancestors.iter().rev().enumerate() {
        let leftmost = i + 1 == ancestors.len();
        match *part {
            Part::Element { type_name, name } => chain.push(Step::Element { type_name, name }),
            Part::Wildcard if leftmost || chain.last() == Some(&Step::AnyDepth) => {
                return Err(SelectorError::MisplacedWildcard);
            }
            Part::Wildcard => chain.push(Step::AnyDepth),
        }
    }
    Ok(Target {
        type_name,
        name: *name,
        chain: Box::leak(chain.into_boxed_slice()),
    })
}

fn parse_part(part: &'static str, framework: Framework) -> Result<Part, SelectorError> {
    let part = part.trim();
    match part {
        "*" => return Ok(Part::Wildcard),
        ":root" => return Err(SelectorError::Unsupported(":root")),
        _ => {}
    }
    let end = part.find(['#', '@', '[']).unwrap_or(part.len());
    let type_name = part[..end].trim();
    if type_name.is_empty() {
        return Err(SelectorError::EmptyType);
    }
    let mut name = Name::Any;
    let mut rest = &part[end..];
    while let Some(marker) = rest.chars().next() {
        let after = &rest[1..];
        let next = after.find(['#', '@', '[']).unwrap_or(after.len());
        match marker {
            '#' if name != Name::Any => return Err(SelectorError::TwoNames),
            '#' => {
                let value = after[..next].trim();
                if value.is_empty() {
                    return Err(SelectorError::EmptyName);
                }
                name = Name::Is(value);
            }
            '@' => return Err(SelectorError::Unsupported("@VisualState")),
            _ => return Err(SelectorError::Unsupported("[index] or [Property=Value]")),
        }
        rest = &after[next..];
    }
    Ok(Part::Element {
        type_name: full_type_name(type_name, framework),
        name,
    })
}

/// The type name XAML Diagnostics reports for a selector's type.
fn full_type_name(type_name: &'static str, framework: Framework) -> &'static str {
    if let Some(short) = type_name.strip_prefix("muxc:") {
        return leak(format!("Microsoft.UI.Xaml.Controls.{short}"));
    }
    if type_name.contains(['.', ':']) {
        return type_name;
    }
    let namespace = framework.namespace();
    if type_name == "Rectangle" {
        return leak(format!("{namespace}.Shapes.Rectangle"));
    }
    leak(format!("{namespace}.Controls.{type_name}"))
}
