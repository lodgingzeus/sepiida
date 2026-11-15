//! The shape of a rule: which element (type, `x:Name`, a required ancestor
//! or parent), which property, and its effect: a palette colour, a layout
//! value, a probe or a measured part.

use crate::shell::layout::Value;

/// A palette colour sepiida.exe sends to the DLL. The DLL paints every
/// target of a role with that role's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Role {
    /// Large background areas: the taskbar, Start, quick settings, the
    /// notification centre.
    Surface = 1,
    /// Highlights; not used by any rule yet.
    Accent = 2,
    /// Cards that sit on a surface (notifications): a step lighter than
    /// `Surface`, painted solid so the text on them stays readable.
    SurfaceRaised = 3,
}

impl Role {
    pub const ALL: [Self; 3] = [Self::Surface, Self::Accent, Self::SurfaceRaised];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.id() == id)
    }
}

/// The look's material, a setting: glass (translucent) or solid. Look
/// rules may hold for one only; colour rules hold for both.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Material {
    #[default]
    Glass = 0,
    Solid = 1,
}

impl Material {
    pub const fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        [Self::Glass, Self::Solid]
            .into_iter()
            .find(|m| m.id() == id)
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Glass => Self::Solid,
            Self::Solid => Self::Glass,
        }
    }
}

/// How a rule's property gets the role's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Replace the value with a `SolidColorBrush` of the colour: opaque
    /// cards (`Role::SurfaceRaised`). Large surfaces are tinted instead,
    /// so the wallpaper still shows through them.
    Solid,
    /// Keep Windows' own `AcrylicBrush` and set its tint to the colour.
    /// Values that are not an `AcrylicBrush` are left alone.
    Tint,
    /// Change nothing; log the property's value chain. For elements that
    /// may cover a styled surface (overlays), to learn whether they do.
    LogOnly,
}

/// What a rule does to its property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// The palette colour of `role`, applied the `paint` way. Always on
    /// while styling.
    Color { paint: Paint, role: Role },
    /// A layout value, set only while the look is on and put back when it
    /// is turned off.
    Set(Value),
    /// Changes nothing: logs the element's layout properties (size,
    /// offset) whenever the look is switched, to learn the geometry a
    /// look must fit. `property` is unused.
    Probe,
    /// Part of a measured layout (`measure`), while the look is on.
    Measure(Part),
}

/// The elements a measured layout reads or sets: values a Windhawk theme
/// computes from captured sizes (`ActualWidth=>name`, `{{…}}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// The glass bar behind the buttons: its `Margin` is computed.
    Bar,
    /// The tray panel: its `ActualWidth` is the tray's width.
    Tray,
    /// The taskbar frame of a centred dock: its `MaxWidth` is computed
    /// from the width of the grid that holds it (its parent) and the
    /// tray's.
    Dock,
    /// Start's pinned list: its `ActualHeight` places the View button.
    PinnedList,
    /// Start's View button: its `RenderTransform` is computed.
    ViewButton,
}

impl Part {
    /// For a part whose value is computed, what it starts as (never
    /// written before it is worked out); `None` for a part only read.
    pub const fn placeholder(self) -> Option<Value> {
        match self {
            Self::Bar => Some(Value::Thickness([0.0; 4])),
            Self::Dock => Some(Value::Length(0.0)),
            Self::ViewButton => Some(Value::Translate { x: 0.0, y: 0.0 }),
            Self::Tray | Self::PinnedList => None,
        }
    }
}

/// The `x:Name` a rule asks of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    /// Any name, or none (a Windhawk selector without `#Name`).
    Any,
    /// Exactly this name; `Is("")` asks for an unnamed element.
    Is(&'static str),
}

impl Name {
    pub fn accepts(self, name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Is(wanted) => wanted == name,
        }
    }
}

/// One ancestor in a rule's chain (Windhawk's `Parent > Child`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The next ancestor up has this type and name, and is its parent's
    /// child at `position` (0-based) when one is given.
    Element {
        type_name: &'static str,
        name: Name,
        position: Option<u32>,
    },
    /// Any number of ancestors, none included (Windhawk's `*`).
    AnyDepth,
}

/// A property value an element must hold locally: `Property` (or an
/// attached `Owner.Property`) parsed as XAML would parse `Value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    pub property: &'static str,
    pub value: &'static str,
}

/// One kind of element to style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// For logs, e.g. "taskbar background".
    pub label: &'static str,
    /// The type XAML Diagnostics reports for the element.
    pub type_name: &'static str,
    /// The element's `x:Name`.
    pub name: Name,
    /// The `x:Name` one of its ancestors must have; tells apart elements
    /// that share a type and name in different parts of the shell.
    pub ancestor: Option<&'static str>,
    /// The `x:Name` its direct parent must have; for an element whose name
    /// is common (`RootGrid`) and whose place is known exactly.
    pub parent: Option<&'static str>,
    /// The ancestors the element must have, nearest first: its parent
    /// matches the first step, and so on up. Empty for no constraint.
    pub chain: &'static [Step],
    /// The element's place among its parent's children (0-based), for a
    /// Windhawk `Type[n]`; `None` for any.
    pub position: Option<u32>,
    /// Property values the element must hold locally (Windhawk's
    /// `Type[Property=Value]`), checked by the DLL when it is found.
    pub conditions: &'static [Condition],
    /// The material a look rule holds for; `None` for both.
    pub material: Option<Material>,
    /// The property the rule changes.
    pub property: &'static str,
    pub effect: Effect,
}

impl Rule {
    /// Whether an element of this type and name is a candidate; the
    /// ancestor is checked separately, since it may arrive later.
    pub fn selects(&self, type_name: &str, name: &str) -> bool {
        self.type_name == type_name && self.name.accepts(name)
    }
}

/// Where a look rule's element is: type, `x:Name`, the name of a required
/// ancestor and of a required direct parent.
pub(super) type Place = (
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
);

/// A rule that marks an element of a measured layout: read for `part`,
/// or its `property` computed for it (`Part::placeholder`).
pub(super) const fn measured(
    label: &'static str,
    type_name: &'static str,
    name: Name,
    property: &'static str,
    part: Part,
) -> Rule {
    Rule {
        label,
        type_name,
        name,
        ancestor: None,
        parent: None,
        chain: &[],
        position: None,
        conditions: &[],
        material: None,
        property,
        effect: Effect::Measure(part),
    }
}

/// A look rule: `property` of the element at `place` gets `value` while
/// the look is on.
pub(super) const fn look(
    label: &'static str,
    (type_name, name, ancestor, parent): Place,
    property: &'static str,
    value: Value,
) -> Rule {
    Rule {
        label,
        type_name,
        name: Name::Is(name),
        ancestor,
        parent,
        chain: &[],
        position: None,
        conditions: &[],
        material: None,
        property,
        effect: Effect::Set(value),
    }
}

/// Position of a rule in the rule set it came from.
pub type RuleId = usize;

/// The type XAML Diagnostics reports for rectangles.
pub const RECTANGLE_TYPE: &str = "Windows.UI.Xaml.Shapes.Rectangle";
pub const BORDER_TYPE: &str = "Windows.UI.Xaml.Controls.Border";
pub const GRID_TYPE: &str = "Windows.UI.Xaml.Controls.Grid";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ids_round_trip_and_unknown_ids_are_rejected() {
        for role in Role::ALL {
            assert_eq!(Role::from_id(role.id()), Some(role));
        }
        assert_eq!(Role::from_id(0), None);
        assert_eq!(Role::from_id(4), None);
        assert_eq!(Role::from_id(u8::MAX), None);
    }

    #[test]
    fn a_rule_selects_on_type_and_name() {
        let rule = Rule {
            label: "test",
            type_name: RECTANGLE_TYPE,
            name: Name::Is("BackgroundFill"),
            ancestor: None,
            parent: None,
            chain: &[],
            position: None,
            conditions: &[],
            material: None,
            property: "Fill",
            effect: Effect::Probe,
        };
        assert!(rule.selects(RECTANGLE_TYPE, "BackgroundFill"));
        assert!(!rule.selects(BORDER_TYPE, "BackgroundFill"));
        assert!(!rule.selects(RECTANGLE_TYPE, "Other"));
        let any = Rule {
            name: Name::Any,
            ..rule
        };
        assert!(any.selects(RECTANGLE_TYPE, "Other"));
        assert!(any.selects(RECTANGLE_TYPE, ""));
        let unnamed = Rule {
            name: Name::Is(""),
            ..rule
        };
        assert!(unnamed.selects(RECTANGLE_TYPE, ""));
        assert!(!unnamed.selects(RECTANGLE_TYPE, "BackgroundFill"));
    }
}
