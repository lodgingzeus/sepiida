//! The elements the rules select, and painting them with their role's
//! colour.
//!
//! The original value is captured when a target is painted, not when it is
//! found: Explorer may replace the value later (a theme or mode change),
//! and restoring must bring back what it set last, not a stale brush.

use std::collections::HashMap;

use sepiida_core::{
    Color,
    shell::{finder::Match, rules::Role, rules::Rule, tree::Handle},
};
use windows::core::IUnknown;

use super::solid::SolidBrushes;
use crate::{
    log::log,
    properties::Xaml,
    xamlom::{IVisualTreeService3, IXamlDiagnostics, InstanceHandle},
};

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

pub struct Styler {
    xaml: Xaml,
    /// The colour asked for each role; kept until a restore.
    colors: HashMap<Role, Color>,
    solid: SolidBrushes,
    targets: HashMap<Match, Target>,
}

impl Styler {
    pub fn new(diagnostics: IXamlDiagnostics, service: IVisualTreeService3) -> Self {
        Self {
            xaml: Xaml::new(diagnostics, service),
            colors: HashMap::new(),
            solid: SolidBrushes::default(),
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
        if self.ensure_brush(rule.role) {
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

    /// Gives the role's brush the role's colour, creating it if needed.
    /// False if the role has no colour yet or the brush refused it.
    fn ensure_brush(&mut self, role: Role) -> bool {
        let Some(&color) = self.colors.get(&role) else {
            return false;
        };
        self.solid.ensure(&self.xaml, role, color)
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
        let Some(brush) = self.solid.handle(target.role) else {
            return;
        };
        let index = target.property_index;
        let current = self.xaml.property(handle, index);
        if !current.is_some_and(|value| self.solid.is_ours(value)) {
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
}
