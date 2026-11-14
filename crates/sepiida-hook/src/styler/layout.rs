//! Look rules: layout values (margins, corners, visibility) set on the
//! elements they select while the look is on, and put back when it is
//! turned off. What to write and how to restore comes from
//! `sepiida_core::shell::layout::plan_write`; this module only makes the
//! calls.
//!
//! The original is read when a value is written, not when the element is
//! found: turning the look off and on again reads it afresh. An acrylic
//! value waits for its role's colour, which also re-tints it later.

use std::collections::HashMap;

use sepiida_core::{
    Color,
    shell::{
        finder::Match,
        layout::{Current, Make, Restore, Value, plan_write},
        rules::{Part, Role},
        tint::Opacity,
        tree::Handle,
    },
};

use super::{acrylic::OwnAcrylic, measured::Measured};
use windows::core::IUnknown;

use crate::{log::log, properties::Xaml, xamlom::InstanceHandle};

struct Target {
    label: &'static str,
    property: &'static str,
    value: Value,
    /// The value comes from the measured layout; not written before it
    /// has been worked out (`ready`).
    measured: bool,
    ready: bool,
    /// Set while our value is on the element.
    applied: Option<Applied>,
}

struct Applied {
    index: u32,
    restore: Restore,
    /// `Restore::Hold`: the original value, held so its handle can be
    /// asked for afresh.
    held: Option<IUnknown>,
    /// The type a parsed value was made as, to make a new one later.
    type_name: Option<String>,
}

#[derive(Default)]
pub struct Layouts {
    on: bool,
    targets: HashMap<Match, Target>,
    /// Each role's colour and tint opacity, for the acrylic values.
    colors: HashMap<Role, (Color, Option<Opacity>)>,
    acrylic: OwnAcrylic,
    /// Elements whose layout is logged on every switch (`Effect::Probe`).
    probes: Vec<(Handle, &'static str)>,
    measured: Measured,
}

/// What a probe logs: an element's size and place.
const PROBED: [&str; 8] = [
    "ActualWidth",
    "ActualHeight",
    "ActualOffset",
    "DesiredSize",
    "RenderSize",
    "Margin",
    "Width",
    "HorizontalAlignment",
];

impl Layouts {
    /// An element a look rule selects appeared. Styled now if the look is
    /// on.
    pub fn found(
        &mut self,
        xaml: &Xaml,
        found: Match,
        label: &'static str,
        property: &'static str,
        value: Value,
    ) {
        if self.targets.contains_key(&found) {
            return;
        }
        log!("{label} found: {:#x}", found.handle);
        self.targets.insert(
            found,
            Target {
                label,
                property,
                value,
                measured: false,
                ready: false,
                applied: None,
            },
        );
        if self.on {
            self.apply(xaml, found);
        }
    }

    /// An element of the measured layout appeared. The frame's margin
    /// becomes a target written once worked out.
    pub fn measure(
        &mut self,
        xaml: &Xaml,
        found: Match,
        label: &'static str,
        property: &'static str,
        part: Part,
    ) {
        log!("{label} found: {:#x}", found.handle);
        self.measured.note(found, part);
        if part == Part::Bar && !self.targets.contains_key(&found) {
            self.targets.insert(
                found,
                Target {
                    label,
                    property,
                    value: Value::Thickness([0.0; 4]),
                    measured: true,
                    ready: false,
                    applied: None,
                },
            );
        }
        if self.on {
            self.relayout(xaml);
        }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Works out the measured layout again and writes it if it changed.
    pub fn relayout(&mut self, xaml: &Xaml) {
        if !self.on {
            return;
        }
        let Some((found, value)) = self.measured.placement(xaml) else {
            return;
        };
        let Some(target) = self.targets.get_mut(&found) else {
            return;
        };
        if target.ready && target.value == value {
            return;
        }
        target.value = value;
        target.ready = true;
        let rewrite = target
            .applied
            .as_ref()
            .and_then(|a| Some((a.index, a.type_name.clone()?)));
        let Some((index, type_name)) = rewrite else {
            self.apply(xaml, found);
            return;
        };
        // Already ours: only the value changes; the original stays recorded.
        let Some(created) = value
            .candidates()
            .first()
            .and_then(|c| xaml.create(&type_name, Some(c)))
        else {
            return;
        };
        let hr = xaml.set_property(found.handle, index, created);
        log!("{} {:#x}: now {value}: {hr}", target.label, found.handle);
    }

    /// An element a probe selects appeared.
    pub fn probe(&mut self, handle: Handle, label: &'static str) {
        if !self.probes.iter().any(|&(h, _)| h == handle) {
            self.probes.push((handle, label));
        }
    }

    /// Elements that left the tree. Nothing to restore on them.
    pub fn forget(&mut self, handles: &[Handle]) {
        self.measured.forget(handles);
        self.probes.retain(|(h, _)| !handles.contains(h));
        self.targets
            .retain(|found, _| !handles.contains(&found.handle));
    }

    /// Turns the look on (writes every value) or off (puts every original
    /// back).
    pub fn set_on(&mut self, xaml: &Xaml, on: bool) {
        self.on = on;
        // In rule order, so that the log reads the same every time.
        let mut found: Vec<Match> = self.targets.keys().copied().collect();
        found.sort_by_key(|m| (m.handle, m.rule));
        log!(
            "look {}: {} target(s)",
            if on { "on" } else { "off" },
            found.len()
        );
        for found in found {
            if on {
                self.apply(xaml, found);
            } else {
                self.restore(xaml, found);
            }
        }
        if on {
            self.relayout(xaml);
        }
        // Read before this switch's layout pass: what the last one left.
        for &(handle, label) in &self.probes {
            log!("{label} {handle:#x}:");
            xaml.log_chain(handle, &PROBED);
        }
    }

    fn apply(&mut self, xaml: &Xaml, found: Match) {
        let Some(target) = self.targets.get(&found) else {
            return;
        };
        if target.applied.is_some() || (target.measured && !target.ready) {
            return;
        }
        let (handle, label, property, value) =
            (found.handle, target.label, target.property, target.value);
        let Some((index, entry)) = xaml.effective(handle, property) else {
            return;
        };
        let current = Current {
            type_name: &entry.type_name,
            source: entry.source,
            value: &entry.value,
        };
        let write = match plan_write(&value, &current) {
            Ok(write) => write,
            Err(skip) => {
                log!("{label} {handle:#x} left alone: {skip}");
                return;
            }
        };
        match &write.make {
            Make::Parse {
                type_name,
                candidates,
            } => {
                for candidate in candidates {
                    let Some(created) = xaml.create(type_name, Some(candidate)) else {
                        continue;
                    };
                    let what = format!("{candidate} ({type_name})");
                    if self.set(xaml, found, index, created, &what, &write.restore) {
                        if let Some(applied) = self
                            .targets
                            .get_mut(&found)
                            .and_then(|t| t.applied.as_mut())
                        {
                            applied.type_name = Some(type_name.clone());
                        }
                        return;
                    }
                }
                log!("{label} {handle:#x}: no form of {value} was taken");
            }
            &Make::Acrylic(role) => {
                let Some(&(color, opacity)) = self.colors.get(&role) else {
                    log!("{label} {handle:#x}: waits for the {role:?} colour");
                    return;
                };
                if let Some(brush) = self.acrylic.brush(xaml, role, color, opacity) {
                    self.set(
                        xaml,
                        found,
                        index,
                        brush,
                        &value.to_string(),
                        &write.restore,
                    );
                }
            }
        }
    }

    /// Sets `value` on the target's property and records how to restore
    /// it. Returns whether XAML took it.
    fn set(
        &mut self,
        xaml: &Xaml,
        found: Match,
        index: u32,
        value: InstanceHandle,
        what: &str,
        restore: &Restore,
    ) -> bool {
        // Read before our value replaces it.
        let held = (*restore == Restore::Hold)
            .then(|| xaml.property(found.handle, index))
            .flatten()
            .and_then(|original| xaml.object(original));
        let Some(target) = self.targets.get_mut(&found) else {
            return false;
        };
        let hr = xaml.set_property(found.handle, index, value);
        log!(
            "{} {:#x}: {} = {what}: {hr}",
            target.label,
            found.handle,
            target.property
        );
        if hr.is_ok() {
            target.applied = Some(Applied {
                index,
                restore: restore.clone(),
                held,
                type_name: None,
            });
        }
        hr.is_ok()
    }

    /// A role's colour or tint opacity changed: re-tints its acrylic, and
    /// writes acrylic values that waited for it if the look is on.
    pub fn set_color(&mut self, xaml: &Xaml, role: Role, color: Color, opacity: Option<Opacity>) {
        self.colors.insert(role, (color, opacity));
        self.acrylic.retint(role, color, opacity);
        if self.on {
            let waiting: Vec<Match> = self
                .targets
                .iter()
                .filter(|(_, t)| t.applied.is_none() && t.value == Value::Acrylic(role))
                .map(|(&found, _)| found)
                .collect();
            for found in waiting {
                self.apply(xaml, found);
            }
        }
    }

    fn restore(&mut self, xaml: &Xaml, found: Match) {
        let Some(target) = self.targets.get_mut(&found) else {
            return;
        };
        // A measured value is worked out afresh next time.
        target.ready = false;
        let Some(applied) = target.applied.take() else {
            return;
        };
        let (handle, index, restore) = (found.handle, applied.index, &applied.restore);
        let hr = match restore {
            Restore::Clear => xaml.clear_property(handle, index),
            Restore::Hold => match applied.held.as_ref().and_then(|o| xaml.handle_of(o)) {
                Some(original) => xaml.set_property(handle, index, original),
                None => {
                    log!("{} {handle:#x}: original not held; clearing", target.label);
                    xaml.clear_property(handle, index)
                }
            },
            Restore::Recreate { type_name, value } => match xaml.create(type_name, Some(value)) {
                Some(original) => xaml.set_property(handle, index, original),
                None => {
                    log!(
                        "{} {handle:#x}: original {value} not recreated; clearing",
                        target.label
                    );
                    xaml.clear_property(handle, index)
                }
            },
        };
        log!("{} {handle:#x}: restored ({restore:?}): {hr}", target.label);
        if hr.is_err() {
            // Kept, so that a later restore tries again.
            target.applied = Some(applied);
        }
    }

    /// Puts every original back and turns the look off. The colours are
    /// forgotten; the brushes are kept for the next time.
    pub fn restore_all(&mut self, xaml: &Xaml) {
        self.colors.clear();
        if self.on || self.targets.values().any(|t| t.applied.is_some()) {
            self.set_on(xaml, false);
        }
    }
}
