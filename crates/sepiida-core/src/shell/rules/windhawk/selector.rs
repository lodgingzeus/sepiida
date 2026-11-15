//! Windhawk target selectors: `Type#Name > Type > * > Type#Name`, several
//! separated by commas. The last part is the element; the others are its
//! ancestors, ` > ` meaning "direct parent of" and `*` any number of
//! ancestors in between. A type without a namespace is a `Controls` type
//! of the theme's framework (`Windows.UI.Xaml.Controls` for the shell,
//! `Microsoft.UI.Xaml.Controls` for File Explorer), `Rectangle` a shape,
//! and `muxc:` stands for `Microsoft.UI.Xaml.Controls.`, as in the
//! Windhawk stylers.
//!
//! `Type[n]` asks for the parent's n-th child (1-based, as in Windhawk).
//! `Type[Property=Value]` asks for a local property value; on the element
//! only (the DLL checks it when the element is found), and in system XAML
//! only, like attached properties. Not supported yet, and reported as
//! such: a condition on an ancestor, `@Group` (visual state) and `:root`.

use std::fmt;

use super::leak;
use crate::shell::{
    framework::Framework,
    rules::{Condition, Name, Step},
};

/// One parsed selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub type_name: &'static str,
    pub name: Name,
    /// Ancestors, nearest first.
    pub chain: &'static [Step],
    /// Place among the parent's children, 0-based.
    pub position: Option<u32>,
    /// Local property values the element must hold.
    pub conditions: &'static [Condition],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorError {
    Empty,
    EmptyType,
    EmptyName,
    TwoNames,
    /// `[0]`, two indices, or an index that is not a number.
    BadIndex,
    /// `[=Value]`, `[Property=]` or a `[` without its `]`.
    BadCondition,
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
            Self::BadIndex => f.write_str("bad child index"),
            Self::BadCondition => f.write_str("bad property condition"),
            Self::MisplacedWildcard => f.write_str("misplaced '*'"),
            Self::Unsupported(what) => write!(f, "{what} is not supported yet"),
        }
    }
}

enum Part {
    Element {
        type_name: &'static str,
        name: Name,
        position: Option<u32>,
        conditions: Vec<Condition>,
    },
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
    let Some((
        Part::Element {
            type_name,
            name,
            position,
            conditions,
        },
        ancestors,
    )) = parts.split_last()
    else {
        return Err(SelectorError::MisplacedWildcard);
    };
    let mut chain = Vec::with_capacity(ancestors.len());
    for (i, part) in ancestors.iter().rev().enumerate() {
        let leftmost = i + 1 == ancestors.len();
        match *part {
            Part::Element {
                type_name,
                name,
                position,
                ref conditions,
            } => {
                if !conditions.is_empty() {
                    return Err(SelectorError::Unsupported(
                        "[Property=Value] on an ancestor",
                    ));
                }
                chain.push(Step::Element {
                    type_name,
                    name,
                    position,
                });
            }
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
        position: *position,
        conditions: Box::leak(conditions.clone().into_boxed_slice()),
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
    let mut position = None;
    let mut conditions = Vec::new();
    let mut rest = &part[end..];
    while let Some(marker) = rest.chars().next() {
        let after = &rest[1..];
        // A bracket runs to its `]`: a value may hold `#` or `@`.
        let next = if marker == '[' {
            after
                .find(']')
                .map(|i| i + 1)
                .ok_or(SelectorError::BadIndex)?
        } else {
            after.find(['#', '@', '[']).unwrap_or(after.len())
        };
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
            _ if after[..next].contains('=') => {
                if framework == Framework::WinUi {
                    return Err(SelectorError::Unsupported("[Property=Value] in WinUI 3"));
                }
                conditions.push(condition(&after[..next - 1])?);
            }
            _ if position.is_some() => return Err(SelectorError::BadIndex),
            _ => position = Some(child_index(&after[..next])?),
        }
        rest = &after[next..];
    }
    Ok(Part::Element {
        type_name: full_type_name(type_name, framework),
        name,
        position,
        conditions,
    })
}

/// `Property=Value` of `[Property=Value]`.
fn condition(inner: &'static str) -> Result<Condition, SelectorError> {
    let (property, value) = inner.split_once('=').ok_or(SelectorError::BadCondition)?;
    let (property, value) = (property.trim(), value.trim());
    if property.is_empty() || value.is_empty() {
        return Err(SelectorError::BadCondition);
    }
    Ok(Condition { property, value })
}

/// `n]` of `[n]`: the 0-based place of a 1-based index.
fn child_index(text: &str) -> Result<u32, SelectorError> {
    let inner = text
        .trim()
        .strip_suffix(']')
        .ok_or(SelectorError::BadIndex)?
        .trim();
    match inner.parse::<u32>() {
        Ok(n) if n >= 1 && inner.bytes().all(|b| b.is_ascii_digit()) => Ok(n - 1),
        _ => Err(SelectorError::BadIndex),
    }
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
