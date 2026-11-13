//! Paints the elements the rules select with their role's colour through
//! XAML Diagnostics, and puts the original values back.
//!
//! One `SolidColorBrush` per role is shared by every target of that role;
//! changing a role's colour changes its brush's `Color`. A brush is created
//! only once a target of its role exists, so XAML threads without targets
//! stay untouched. Its colour is set through the diagnostics API
//! (`Windows.UI.Color` created from a string) or, if XAML refuses that,
//! through `ISolidColorBrush::put_Color`; whichever works first is kept.
//!
//! The original value is captured when a target is painted, not when it is
//! found: Explorer may replace the value later (a theme or mode change),
//! and restoring must bring back what it set last, not a stale brush.

use std::collections::HashMap;

use sepiida_core::{
    Color,
    shell::{finder::Match, protocol::xaml_color, rules::Role, rules::Rule, tree::Handle},
};
use windows::core::{IUnknown, Interface};

use crate::{
    log::log,
    properties::{Xaml, log_failure},
    xamlom::{ISolidColorBrush, IVisualTreeService3, IXamlDiagnostics, InstanceHandle},
};

const BRUSH_TYPE: &str = "Windows.UI.Xaml.Media.SolidColorBrush";
const COLOR_TYPE: &str = "Windows.UI.Color";

struct Target {
    label: &'static str,
    property_index: u32,
    role: Role,
    /// The value before our brush replaced it: `None` until first painted,
    /// `Some(None)` if the property had no value.
    original: Option<Option<Original>>,
    styled: bool,
}

struct Original {
    handle: InstanceHandle,
    /// A strong reference to the value. XAML Diagnostics holds handles
    /// weakly: once our brush replaces this one nothing else keeps it
    /// alive, and `handle` goes stale (ERROR_NOT_FOUND on restore).
    object: Option<IUnknown>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorPath {
    Diagnostics,
    Interface,
}

pub struct Styler {
    xaml: Xaml,
    /// The colour asked for each role; kept until a restore.
    colors: HashMap<Role, Color>,
    brushes: HashMap<Role, InstanceHandle>,
    color_path: Option<ColorPath>,
    targets: HashMap<Match, Target>,
}

impl Styler {
    pub fn new(diagnostics: IXamlDiagnostics, service: IVisualTreeService3) -> Self {
        Self {
            xaml: Xaml::new(diagnostics, service),
            colors: HashMap::new(),
            brushes: HashMap::new(),
            color_path: None,
            targets: HashMap::new(),
        }
    }

    /// An element `rule` styles appeared. Paints it if its role has a
    /// colour.
    pub fn found(&mut self, found: Match, rule: &'static Rule) {
        if self.targets.contains_key(&found) {
            return;
        }
        let handle = found.handle;
        log!("{} found: {handle:#x}", rule.label);
        let Some(property_index) = self.xaml.property_index(handle, rule.property) else {
            return;
        };
        log!("{} index {property_index}", rule.property);
        self.targets.insert(
            found,
            Target {
                label: rule.label,
                property_index,
                role: rule.role,
                original: None,
                styled: false,
            },
        );
        if self.colors.contains_key(&rule.role) && self.ensure_brush(rule.role) {
            self.paint(found);
        }
    }

    /// Elements that left the tree. Nothing to restore on them.
    pub fn forget(&mut self, handles: &[Handle]) {
        self.targets.retain(|found, target| {
            let keep = !handles.contains(&found.handle);
            if !keep {
                log!("{} removed: {:#x}", target.label, found.handle);
            }
            keep
        });
    }

    /// Paints every target of `role` in `color`. If the brush cannot take
    /// the colour, nothing is painted and the previous colour (if any)
    /// stays: an uncoloured brush would make the surface transparent.
    pub fn set_color(&mut self, role: Role, color: Color) {
        self.colors.insert(role, color);
        let targets = self.targets_of(role);
        if targets.is_empty() {
            log!("no target for {role:?} on this thread; colour kept for later");
            return;
        }
        if !self.ensure_brush(role) {
            return;
        }
        for found in targets {
            self.paint(found);
        }
    }

    pub fn restore_all(&mut self) {
        self.colors.clear();
        let styled: Vec<Match> = self
            .targets
            .iter()
            .filter(|(_, target)| target.styled)
            .map(|(&found, _)| found)
            .collect();
        for found in styled {
            self.restore(found);
        }
    }

    fn targets_of(&self, role: Role) -> Vec<Match> {
        self.targets
            .iter()
            .filter(|(_, target)| target.role == role)
            .map(|(&found, _)| found)
            .collect()
    }

    fn restore(&mut self, found: Match) {
        let Some(target) = self.targets.get(&found) else {
            return;
        };
        let (handle, index) = (found.handle, target.property_index);
        let hr = match target.original.as_ref().and_then(Option::as_ref) {
            Some(original) => {
                let value = self.live_handle(original);
                self.xaml.set_property(handle, index, value)
            }
            None => self.xaml.clear_property(handle, index),
        };
        log!("restore {handle:#x}: {hr}");
        if hr.is_ok()
            && let Some(target) = self.targets.get_mut(&found)
        {
            target.styled = false;
        }
    }

    /// A handle to the original value that is valid now: asked for afresh
    /// from the object we hold, since the recorded handle may have gone
    /// stale.
    fn live_handle(&self, original: &Original) -> InstanceHandle {
        let Some(object) = &original.object else {
            return original.handle;
        };
        self.xaml.handle_of(object).unwrap_or_else(|| {
            log!("trying the recorded handle");
            original.handle
        })
    }

    fn paint(&mut self, found: Match) {
        let handle = found.handle;
        let Some(target) = self.targets.get(&found) else {
            return;
        };
        let Some(&brush) = self.brushes.get(&target.role) else {
            return;
        };
        let index = target.property_index;
        let current = self.xaml.property(handle, index);
        if current != Some(brush) {
            // Ours was never set, or Explorer has since put its own value
            // there: that value is the one to restore.
            let original = current.map(|value| Original {
                handle: value,
                object: self.xaml.object(value),
            });
            log!(
                "original value of {handle:#x}: {current:#x?} (held: {}){}",
                original.as_ref().is_some_and(|o| o.object.is_some()),
                if target.styled {
                    "; Explorer replaced our brush"
                } else {
                    ""
                },
            );
            if let Some(target) = self.targets.get_mut(&found) {
                target.original = Some(original);
            }
        }
        let hr = self.xaml.set_property(handle, index, brush);
        log!("set property {index} on {handle:#x}: {hr}");
        if hr.is_err() {
            return;
        }
        if let Some(target) = self.targets.get_mut(&found) {
            target.styled = true;
        }
        // Read back: did XAML keep our brush?
        let now = self.xaml.property(handle, index);
        log!(
            "{handle:#x} now has {now:#x?} (our brush {brush:#x}): {}",
            if now == Some(brush) {
                "kept"
            } else {
                "NOT kept"
            }
        );
    }

    /// Creates the role's brush if needed and gives it the role's colour.
    /// Returns whether the brush now has that colour.
    fn ensure_brush(&mut self, role: Role) -> bool {
        let Some(&color) = self.colors.get(&role) else {
            return false;
        };
        let brush = match self.brushes.get(&role) {
            Some(&brush) => brush,
            None => match self.xaml.create(BRUSH_TYPE, None) {
                Some(brush) => {
                    self.brushes.insert(role, brush);
                    brush
                }
                None => return false,
            },
        };
        let ok = match self.color_path {
            Some(ColorPath::Diagnostics) => self.color_by_diagnostics(brush, color),
            Some(ColorPath::Interface) => self.color_by_interface(brush, color),
            None => {
                if self.color_by_diagnostics(brush, color) {
                    self.color_path = Some(ColorPath::Diagnostics);
                    true
                } else if self.color_by_interface(brush, color) {
                    self.color_path = Some(ColorPath::Interface);
                    true
                } else {
                    false
                }
            }
        };
        log!(
            "{role:?} brush colour {color} via {:?}: {ok}",
            self.color_path
        );
        if ok {
            // Read back what XAML now reports for the colour.
            self.xaml.chain_index(brush, "Color");
        }
        ok
    }

    fn color_by_diagnostics(&self, brush: InstanceHandle, color: Color) -> bool {
        let Some(index) = self.xaml.property_index(brush, "Color") else {
            return false;
        };
        let Some(value) = self.xaml.create(COLOR_TYPE, Some(&xaml_color(color))) else {
            return false;
        };
        log_failure(
            "SetProperty(Color)",
            self.xaml.set_property(brush, index, value),
        )
    }

    fn color_by_interface(&self, brush: InstanceHandle, color: Color) -> bool {
        let Some(object) = self.xaml.object(brush) else {
            return false;
        };
        let brush: ISolidColorBrush = match object.cast() {
            Ok(brush) => brush,
            Err(error) => {
                log!("brush is not an ISolidColorBrush: {error}");
                return false;
            }
        };
        let value = windows::UI::Color {
            A: 0xff,
            R: color.r,
            G: color.g,
            B: color.b,
        };
        // SAFETY: plain call on a live interface.
        let hr = unsafe { brush.put_Color(value) };
        log_failure("ISolidColorBrush::put_Color", hr)
    }
}
