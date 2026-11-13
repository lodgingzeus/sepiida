//! What the hook DLL styles, as data: which elements (type, `x:Name` and a
//! required ancestor), which property, and which palette colour goes there.
//! New surfaces are added as rules here rather than as code in the DLL.

/// A palette colour sepiida.exe sends to the DLL. The DLL paints every
/// target of a role with that role's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Role {
    /// Large background areas: the taskbar, later Start and the
    /// notification centre.
    Surface = 1,
    /// Highlights; not used by any rule yet.
    Accent = 2,
}

impl Role {
    pub const ALL: [Self; 2] = [Self::Surface, Self::Accent];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.id() == id)
    }
}

/// One kind of element to style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// For logs, e.g. "taskbar background".
    pub label: &'static str,
    /// The type XAML Diagnostics reports for the element.
    pub type_name: &'static str,
    /// The element's `x:Name`.
    pub name: &'static str,
    /// The `x:Name` one of its ancestors must have; tells apart elements
    /// that share a type and name in different parts of the shell.
    pub ancestor: Option<&'static str>,
    /// The property set to a brush of the role's colour.
    pub property: &'static str,
    pub role: Role,
}

impl Rule {
    /// Whether an element of this type and name is a candidate; the
    /// ancestor is checked separately, since it may arrive later.
    pub fn selects(&self, type_name: &str, name: &str) -> bool {
        self.type_name == type_name && self.name == name
    }
}

/// Position of a rule in the rule set it came from.
pub type RuleId = usize;

/// The type XAML Diagnostics reports for rectangles.
pub const RECTANGLE_TYPE: &str = "Windows.UI.Xaml.Shapes.Rectangle";
/// The root of one taskbar's own tree. The hover flyout also contains a
/// `BackgroundFill` rectangle, but not under this element.
pub const TASKBAR_FRAME_NAME: &str = "TaskbarFrame";

/// The rectangle that paints the taskbar background (Windows 11 23H2 and
/// 24H2: `TaskbarFrame > Grid#RootGrid > TaskbarBackground > Grid >
/// Rectangle#BackgroundFill`).
pub const TASKBAR_BACKGROUND: Rule = Rule {
    label: "taskbar background",
    type_name: RECTANGLE_TYPE,
    name: "BackgroundFill",
    ancestor: Some(TASKBAR_FRAME_NAME),
    property: "Fill",
    role: Role::Surface,
};

/// Everything styled in Explorer's taskbar.
pub const TASKBAR_RULES: &[Rule] = &[TASKBAR_BACKGROUND];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ids_round_trip_and_unknown_ids_are_rejected() {
        for role in Role::ALL {
            assert_eq!(Role::from_id(role.id()), Some(role));
        }
        assert_eq!(Role::from_id(0), None);
        assert_eq!(Role::from_id(3), None);
        assert_eq!(Role::from_id(u8::MAX), None);
    }

    #[test]
    fn a_rule_selects_on_type_and_name() {
        assert!(TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects("Windows.UI.Xaml.Controls.Border", "BackgroundFill"));
        assert!(!TASKBAR_BACKGROUND.selects(RECTANGLE_TYPE, "Other"));
    }
}
