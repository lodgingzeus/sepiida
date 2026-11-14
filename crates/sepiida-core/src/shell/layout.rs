//! Layout values a look rule sets (margins, corners, lengths, visibility),
//! and the decision how one is written and later put back.
//!
//! The DLL reads a property's entry from XAML's property chain: its type
//! name, where its value comes from, and the value as XAML formats it.
//! `plan_write` turns that and the rule's value into the strings
//! `CreateInstance` takes, or into a reason to leave the property alone.
//! Kept here, free of Windows calls, so that it is tested on Linux.

use std::fmt;

use super::rules::Role;

/// Largest length, in effective pixels, a value may use. Larger numbers
/// are never a sensible layout and would hint at a broken rule.
pub const MAX_LENGTH: f32 = 4096.0;

/// The four sides XAML lists for a `Thickness` (left, top, right, bottom)
/// or the four corners of a `CornerRadius` (top-left, top-right,
/// bottom-right, bottom-left).
pub type Sides = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Collapsed,
}

impl Visibility {
    const fn name(self) -> &'static str {
        match self {
            Self::Visible => "Visible",
            Self::Collapsed => "Collapsed",
        }
    }

    const fn number(self) -> u8 {
        match self {
            Self::Visible => 0,
            Self::Collapsed => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Left,
    Center,
    Right,
    Stretch,
}

impl HorizontalAlignment {
    const fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
            Self::Stretch => "Stretch",
        }
    }

    const fn number(self) -> u8 {
        self as u8
    }
}

/// A value a look rule writes to one property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// `Margin`, `Padding`, `BorderThickness`.
    Thickness(Sides),
    /// `CornerRadius` of a `Border`, `Grid` or control.
    CornerRadius(Sides),
    /// A single `Double`: `Width`, `Height`, `RadiusX`, `Opacity`.
    Length(f32),
    /// `Width` or `Height` sized to the content (`Auto`, NaN in code).
    Auto,
    HorizontalAlignment(HorizontalAlignment),
    Visibility(Visibility),
    /// A new acrylic brush, made by the DLL and tinted with the role's
    /// colour: `Background` or `Fill` of a surface the look adds.
    Acrylic(Role),
}

impl Value {
    /// The last part of the type name XAML's property chain reports for a
    /// property that takes this value (`Windows.UI.Xaml.Thickness`,
    /// `Windows.Foundation.Double`). Compared as a suffix, so the
    /// namespace does not have to be known in advance.
    pub const fn type_suffix(&self) -> &'static str {
        match self {
            Self::Thickness(_) => ".Thickness",
            Self::CornerRadius(_) => ".CornerRadius",
            Self::Length(_) | Self::Auto => ".Double",
            Self::HorizontalAlignment(_) => ".HorizontalAlignment",
            Self::Visibility(_) => ".Visibility",
            Self::Acrylic(_) => ".Brush",
        }
    }

    /// Whether every number is finite and within `±MAX_LENGTH`; corner
    /// radii and lengths must also not be negative.
    pub fn is_valid(&self) -> bool {
        let fits = |n: f32| n.is_finite() && n.abs() <= MAX_LENGTH;
        match *self {
            Self::Thickness(sides) => sides.into_iter().all(fits),
            Self::CornerRadius(sides) => sides.into_iter().all(|n| fits(n) && n >= 0.0),
            Self::Length(n) => fits(n) && n >= 0.0,
            Self::Auto | Self::HorizontalAlignment(_) | Self::Visibility(_) | Self::Acrylic(_) => {
                true
            }
        }
    }

    /// The strings to try with `CreateInstance`, best first. XAML's docs do
    /// not say whether an enum value is given by name or by number, so a
    /// visibility offers both.
    pub fn candidates(&self) -> Vec<String> {
        match *self {
            Self::Thickness(sides) | Self::CornerRadius(sides) => vec![format_sides(sides)],
            Self::Length(n) => vec![format_number(n)],
            Self::Visibility(v) => vec![v.name().to_owned(), v.number().to_string()],
            Self::HorizontalAlignment(a) => vec![a.name().to_owned(), a.number().to_string()],
            // XAML markup writes "Auto"; whether `CreateInstance` takes it
            // or only NaN is not documented.
            Self::Auto => vec!["Auto".to_owned(), "NaN".to_owned()],
            // Not made from a string (`Make::Acrylic`).
            Self::Acrylic(_) => Vec::new(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Visibility(v) => f.write_str(v.name()),
            Self::HorizontalAlignment(a) => f.write_str(a.name()),
            Self::Auto => f.write_str("Auto"),
            Self::Acrylic(role) => write!(f, "acrylic in the {role:?} colour"),
            _ => f.write_str(&self.candidates().concat()),
        }
    }
}

/// A number the way XAML writes it: no trailing `.0`, `.` as the decimal
/// separator whatever the locale.
fn format_number(n: f32) -> String {
    // `0.0 == -0.0`, so this also turns -0 into 0.
    if n == 0.0 {
        return "0".to_owned();
    }
    n.to_string()
}

fn format_sides(sides: Sides) -> String {
    sides.map(format_number).join(",")
}

/// Where a property's current value comes from, as XAML's property chain
/// reports it (`BaseValueSource` in `xamlom.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    /// Set on the element itself (`BaseValueSourceLocal`).
    Local,
    /// Set by an animation, which overrides a local value.
    Animation,
    /// Set by a visual state, which changes as the element is used.
    VisualState,
    /// The default, a style, a template, inheritance and the rest: nothing
    /// to write back, clearing our value brings it back.
    Other,
}

impl ValueSource {
    const LOCAL: i32 = 4;
    const ANIMATION: i32 = 12;
    const VISUAL_STATE: i32 = 14;

    pub const fn from_id(id: i32) -> Self {
        match id {
            Self::LOCAL => Self::Local,
            Self::ANIMATION => Self::Animation,
            Self::VISUAL_STATE => Self::VisualState,
            _ => Self::Other,
        }
    }
}

/// A property's effective entry in the property chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Current<'a> {
    /// The property's type, e.g. `Windows.UI.Xaml.Thickness`.
    pub type_name: &'a str,
    pub source: ValueSource,
    /// The value as XAML formats it, e.g. `0,4,0,4`.
    pub value: &'a str,
}

/// How to put a property back once the look is turned off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restore {
    /// It had a local value: create it again from XAML's own string and
    /// set it. (A handle to the old value could go stale; the string
    /// cannot.)
    Recreate { type_name: String, value: String },
    /// It had a local value the chain gives no string for (XAML leaves
    /// `CornerRadius` blank, seen on build 26200): the DLL keeps the value
    /// object it reads before writing and sets that back.
    Hold,
    /// It had no local value: clearing ours brings the style's back.
    Clear,
}

/// How the DLL makes the value it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Make {
    /// `CreateInstance` with the property's type name, as the chain
    /// reports it, and each value string in turn, best first.
    Parse {
        type_name: String,
        candidates: Vec<String>,
    },
    /// The DLL's own acrylic brush for the role.
    Acrylic(Role),
}

/// How to write a look rule's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub make: Make,
    pub restore: Restore,
}

/// Why a look rule leaves a property alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// The rule's value is out of range.
    Invalid,
    /// The property's type is not the one the value is for.
    WrongType {
        expected: &'static str,
        found: String,
    },
    /// An animation or a visual state sets the property: a local value
    /// would either be overridden or fight it as the element is used.
    Driven(ValueSource),
    /// The property holds a local object (a brush). It
    /// cannot be made again from the chain's string, so it could not be
    /// put back.
    LocalObject,
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => f.write_str("the rule's value is out of range"),
            Self::WrongType { expected, found } => {
                write!(f, "the property is a {found}, not a *{expected}")
            }
            Self::Driven(source) => write!(f, "the property is set by {source:?}"),
            Self::LocalObject => f.write_str("the property holds a local object"),
        }
    }
}

/// Decides how to write `value` to a property whose chain entry is
/// `current`, and how to put it back.
pub fn plan_write(value: &Value, current: &Current<'_>) -> Result<Write, Skip> {
    if !value.is_valid() {
        return Err(Skip::Invalid);
    }
    let expected = value.type_suffix();
    if !current.type_name.ends_with(expected) {
        return Err(Skip::WrongType {
            expected,
            found: current.type_name.to_owned(),
        });
    }
    let restore = match current.source {
        ValueSource::Animation | ValueSource::VisualState => {
            return Err(Skip::Driven(current.source));
        }
        ValueSource::Local if matches!(value, Value::Acrylic(_)) => {
            return Err(Skip::LocalObject);
        }
        ValueSource::Local if current.value.is_empty() => Restore::Hold,
        ValueSource::Local => Restore::Recreate {
            type_name: current.type_name.to_owned(),
            value: current.value.to_owned(),
        },
        ValueSource::Other => Restore::Clear,
    };
    let make = match *value {
        Value::Acrylic(role) => Make::Acrylic(role),
        _ => Make::Parse {
            type_name: current.type_name.to_owned(),
            candidates: value.candidates(),
        },
    };
    Ok(Write { make, restore })
}

#[cfg(test)]
mod tests;
