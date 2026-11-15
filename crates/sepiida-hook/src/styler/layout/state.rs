//! The look's targets and its switches: on/off, material, colours, the
//! measured layout.

use std::{collections::HashMap, time::Instant};

use sepiida_core::{
    Color,
    shell::{
        finder::Match,
        layout::{Restore, Value},
        rewrites::{Rewrites, Verdict, size_changed},
        rules::{Material, Part, Role},
        tint::Opacity,
        tree::Handle,
    },
};
use windows::core::IUnknown;

use super::{held::Thinned, sizes::Sizes};
use crate::{
    log::log,
    properties::Xaml,
    styler::{
        acrylic::OwnAcrylic, attached, blur::BlurBrushes, measured::Measured, solid::SolidBrushes,
        transparent::TransparentBrush,
    },
    xamlom::InstanceHandle,
};

pub(super) struct Target {
    pub(super) label: &'static str,
    pub(super) property: &'static str,
    pub(super) value: Value,
    /// The material the rule holds for; `None` for both.
    pub(super) material: Option<Material>,
    /// The value comes from the measured layout; not written before it
    /// has been worked out (`ready`).
    pub(super) measured: bool,
    pub(super) ready: bool,
    /// Set while our value is on the element.
    pub(super) applied: Option<Applied>,
    /// An animation or visual state held the property when we tried:
    /// tried again on tree activity and on every "look on", logged once.
    pub(super) held_back: bool,
}

pub(super) struct Applied {
    pub(super) slot: Slot,
    pub(super) restore: Restore,
    /// `Restore::Hold`: the original value, held so its handle can be
    /// asked for afresh.
    pub(super) held: Option<IUnknown>,
    /// The type a parsed value was made as, to make a new one later.
    pub(super) type_name: Option<String>,
    /// The value we set, to notice when Windows puts its own back.
    pub(super) ours: InstanceHandle,
    /// A size we parsed (`markup::SIZE_PROPERTIES`): the number, to compare
    /// with what the element holds, and the text, to make it again.
    pub(super) size: Option<(f64, String)>,
    /// Our re-writes after Windows replaced the value.
    pub(super) rewrites: Rewrites,
}

/// Where our value went.
pub(super) enum Slot {
    /// The property's index in XAML Diagnostics' property chain.
    Chain(u32),
    /// An attached property, set through `DependencyObject`.
    Attached(attached::Written),
}

impl Slot {
    pub(super) fn index(&self) -> Option<u32> {
        match *self {
            Self::Chain(index) => Some(index),
            Self::Attached(_) => None,
        }
    }
}

#[derive(Default)]
pub struct Layouts {
    pub(super) on: bool,
    /// Targets of the other material are not written.
    pub(super) material: Material,
    pub(super) targets: HashMap<Match, Target>,
    /// Each role's colour and tint opacity, for the acrylic values.
    pub(super) colors: HashMap<Role, (Color, Option<Opacity>)>,
    pub(super) acrylic: OwnAcrylic,
    pub(super) transparent: TransparentBrush,
    /// The solid material's brushes, one per role.
    pub(super) solid: SolidBrushes,
    pub(super) blur: BlurBrushes,
    /// Elements whose layout is logged on every switch (`Effect::Probe`).
    pub(super) probes: Vec<(Handle, &'static str)>,
    pub(super) measured: Measured,
    pub(super) sizes: Sizes,
    /// Held acrylics whose luminosity we lowered, by brush handle.
    pub(super) thinned: HashMap<InstanceHandle, Thinned>,
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
        (value, material): (Value, Option<Material>),
    ) {
        if self.targets.contains_key(&found) {
            return;
        }
        // Two rules on one property would stack: the second would save the
        // first one's value as the original. As in Windhawk, the later rule
        // (higher id) wins; the earlier one's value is put back first.
        // Every one whose material meets this rule's (a pair has two).
        let others: Vec<(Match, &'static str)> = self
            .targets
            .iter()
            .filter(|(m, t)| {
                m.handle == found.handle && t.property == property && overlap(t.material, material)
            })
            .map(|(&m, t)| (m, t.label))
            .collect();
        if let Some(&(_, kept)) = others.iter().find(|(m, _)| m.rule > found.rule) {
            log!(
                "{label} {:#x}: {property} already set by {kept}; left to it",
                found.handle
            );
            return;
        }
        for (other, kept) in others {
            if !self.drop_target(xaml, other) {
                log!(
                    "{label} {:#x}: {property} kept by {kept} (not put back)",
                    found.handle
                );
                return;
            }
            log!(
                "{label} {:#x}: {property} taken over from {kept}",
                found.handle
            );
        }
        log!("{label} found: {:#x}", found.handle);
        self.targets.insert(
            found,
            Target {
                label,
                property,
                value,
                material,
                measured: false,
                ready: false,
                applied: None,
                held_back: false,
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
                    material: None,
                    measured: true,
                    ready: false,
                    applied: None,
                    held_back: false,
                },
            );
        }
        if self.on {
            self.relayout(xaml);
        }
    }

    /// Sets our brush again where Windows put its own brush back (a
    /// surface reopening, a theme switch, a toast built before its brush),
    /// holding Windows' new brush as the one to restore; and our size where
    /// Windows' code set its own (`recheck_size`). Cheap: one property read
    /// per applied brush or size.
    pub fn recheck(&mut self, xaml: &Xaml) {
        if !self.on {
            return;
        }
        for (found, target) in &mut self.targets {
            let Some(applied) = target.applied.as_mut() else {
                continue;
            };
            if applied.size.is_some() {
                recheck_size(xaml, &mut self.sizes, *found, target);
                continue;
            }
            if !is_brush(target.value) {
                continue;
            }
            let Some(index) = applied.slot.index() else {
                continue;
            };
            let Some(now) = xaml.property(found.handle, index) else {
                continue;
            };
            if now == applied.ours {
                continue;
            }
            applied.held = xaml.object(now);
            applied.restore = Restore::Hold;
            let hr = xaml.set_property(found.handle, index, applied.ours);
            log!(
                "{} {:#x}: Windows put {now:#x} back; ours again: {hr}",
                target.label,
                found.handle
            );
        }
        self.retry_held_back(xaml);
    }

    /// Tries again where an animation or visual state held the property,
    /// e.g. the quick settings panel's background, which Windows animates
    /// on a light/dark switch and may hold until the flyout is shown again.
    fn retry_held_back(&mut self, xaml: &Xaml) {
        let waiting: Vec<Match> = self
            .targets
            .iter()
            .filter(|(_, t)| t.held_back && t.applied.is_none())
            .map(|(&m, _)| m)
            .collect();
        for found in waiting {
            self.apply(xaml, found);
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
            .and_then(|a| Some((a.slot.index()?, a.type_name.clone()?)));
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
        // The agent sends "on" with every colour update; targets found
        // while on are styled as they appear, so a repeat only retries
        // those an animation held back.
        if on && self.on {
            self.retry_held_back(xaml);
            return;
        }
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
        } else {
            self.restore_thinned(xaml);
        }
        // Read before this switch's layout pass: what the last one left.
        for &(handle, label) in &self.probes {
            log!("{label} {handle:#x}:");
            xaml.log_chain(handle, &PROBED);
        }
    }

    /// Switches the material: with the look on, every target of the old
    /// one is put back first (so none of ours is taken for an original),
    /// then the new one's are written.
    pub fn set_material(&mut self, xaml: &Xaml, material: Material) {
        if material == self.material {
            return;
        }
        log!("look material: {material:?}");
        let leaving: Vec<Match> = self
            .targets
            .iter()
            .filter(|(_, t)| t.applied.is_some() && !holds(t.material, material))
            .map(|(&m, _)| m)
            .collect();
        for found in leaving {
            self.restore(xaml, found);
        }
        if material != Material::Glass {
            self.restore_thinned(xaml);
        }
        self.material = material;
        if self.on {
            let mut arriving: Vec<Match> = self
                .targets
                .iter()
                .filter(|(_, t)| t.applied.is_none() && t.material == Some(material))
                .map(|(&m, _)| m)
                .collect();
            arriving.sort_by_key(|m| (m.handle, m.rule));
            for found in arriving {
                self.apply(xaml, found);
            }
        }
    }

    /// A role's colour or tint opacity changed: re-tints its acrylic, and
    /// writes acrylic values that waited for it if the look is on.
    pub fn set_color(&mut self, xaml: &Xaml, role: Role, color: Color, opacity: Option<Opacity>) {
        self.colors.insert(role, (color, opacity));
        self.acrylic.retint(role, color, opacity);
        self.blur.set_color(role, color);
        if self.solid.handle(role).is_some() {
            self.solid.ensure(xaml, role, color);
        }
        if self.on {
            let waiting: Vec<Match> = self
                .targets
                .iter()
                .filter(|(_, t)| t.applied.is_none() && waits_for(t.value, role))
                .map(|(&found, _)| found)
                .collect();
            for found in waiting {
                self.apply(xaml, found);
            }
        }
    }
}

/// Writes our size again if Windows' code replaced it, holding Windows'
/// value as the one to restore; gives up on a value Windows keeps setting
/// (`rewrites`).
fn recheck_size(xaml: &Xaml, sizes: &mut Sizes, found: Match, target: &mut Target) {
    let (label, property) = (target.label, target.property);
    let Some(applied) = target.applied.as_mut() else {
        return;
    };
    let (Some((ours, text)), Slot::Chain(index), Some(type_name)) = (
        applied.size.clone(),
        &applied.slot,
        applied.type_name.clone(),
    ) else {
        return;
    };
    let now = match sizes.local(xaml, found.handle, property) {
        Ok(now) => now,
        Err(why) => {
            log!("{label} {:#x}: {property} not read: {why}", found.handle);
            applied.size = None;
            return;
        }
    };
    if !size_changed(ours, now) {
        return;
    }
    let shown = now.map_or("no value".to_owned(), |n| n.to_string());
    match applied.rewrites.decide(Instant::now()) {
        Verdict::Leave => return,
        Verdict::GiveUp => {
            log!(
                "{label} {:#x}: Windows keeps setting {property} ({shown}); left to it",
                found.handle
            );
            return;
        }
        Verdict::Rewrite => {}
    }
    applied.restore = match now {
        Some(n) => Restore::Recreate {
            type_name: type_name.clone(),
            value: n.to_string(),
        },
        None => Restore::Clear,
    };
    let Some(created) = xaml.create(&type_name, Some(&text)) else {
        return;
    };
    let hr = xaml.set_property(found.handle, *index, created);
    if hr.is_ok() {
        applied.ours = created;
    }
    log!(
        "{label} {:#x}: Windows set {property} to {shown}; ours ({text}) again: {hr}",
        found.handle
    );
}

/// Whether a look value is made only once `role` has a colour.
pub(super) fn waits_for(value: Value, role: Role) -> bool {
    match value {
        Value::Acrylic(r) | Value::Solid(r) | Value::Blur { role: r, .. } => r == role,
        _ => false,
    }
}

/// Whether a look value is a brush object of ours, which Windows may
/// replace with its own.
pub(super) fn is_brush(value: Value) -> bool {
    matches!(
        value,
        Value::Acrylic(_)
            | Value::Transparent
            | Value::Solid(_)
            | Value::Xaml(_)
            | Value::Blur { .. }
    )
}

/// Whether a rule for `rule` holds while the material is `now`.
pub(super) fn holds(rule: Option<Material>, now: Material) -> bool {
    rule.is_none_or(|m| m == now)
}

/// Whether two rules can hold at the same time (for some material).
pub(super) fn overlap(a: Option<Material>, b: Option<Material>) -> bool {
    a.is_none() || b.is_none() || a == b
}
