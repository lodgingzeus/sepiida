//! Writes a look rule's value as `plan_write` says.

use sepiida_core::shell::{
    finder::Match,
    held_glass::thins_held,
    layout::{Current, Make, Restore, Skip, Value, ValueSource, plan_write, translate_document},
    markup::{SIZE_PROPERTIES, is_attached},
    rewrites::Rewrites,
};

/// The type XAML Diagnostics reports for a size.
const DOUBLE_TYPE: &str = "Windows.Foundation.Double";

use super::state::{Applied, Layouts, Slot, holds};
use crate::{
    log::log,
    properties::Xaml,
    styler::{attached, markup},
    xamlom::InstanceHandle,
};

impl Layouts {
    pub(super) fn apply(&mut self, xaml: &Xaml, found: Match) {
        let Some(target) = self.targets.get(&found) else {
            return;
        };
        if target.applied.is_some()
            || (target.measured && !target.ready)
            || !holds(target.material, self.material)
        {
            return;
        }
        let (handle, label, property, value) =
            (found.handle, target.label, target.property, target.value);
        if is_attached(property) {
            self.apply_attached(xaml, found);
            return;
        }
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
                let driven = matches!(skip, Skip::Driven(ValueSource::Animation));
                let first = self
                    .targets
                    .get_mut(&found)
                    .is_none_or(|t| !std::mem::replace(&mut t.held_back, driven));
                if first {
                    let later = if driven { "; tried again later" } else { "" };
                    log!("{label} {handle:#x} left alone: {skip}{later}");
                }
                if thins_held(&value, &skip) {
                    self.thin_held(xaml, label, handle, &entry.value);
                }
                return;
            }
        };
        if let Some(target) = self.targets.get_mut(&found)
            && std::mem::take(&mut target.held_back)
        {
            log!("{label} {handle:#x}: no longer held; writing {property}");
        }
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
                            applied.size = (type_name == DOUBLE_TYPE
                                && SIZE_PROPERTIES.contains(&property))
                            .then(|| candidate.parse::<f64>().ok())
                            .flatten()
                            .map(|n| (n, candidate.clone()));
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
            &Make::Solid(role) => {
                let Some(&(color, _)) = self.colors.get(&role) else {
                    log!("{label} {handle:#x}: waits for the {role:?} colour");
                    return;
                };
                if !self.solid.ensure(xaml, role, color) {
                    log!("{label} {handle:#x}: no solid brush");
                    return;
                }
                if let Some(brush) = self.solid.handle(role) {
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
            Make::Transparent => match self.transparent.handle(xaml) {
                Some(brush) => {
                    self.set(xaml, found, index, brush, "transparent", &write.restore);
                }
                None => log!("{label} {handle:#x}: no transparent brush"),
            },
            &Make::Blur {
                amount,
                role,
                opacity,
            } => {
                let Some(&(color, _)) = self.colors.get(&role) else {
                    log!("{label} {handle:#x}: waits for the {role:?} colour");
                    return;
                };
                let made = self
                    .blur
                    .brush(xaml, handle, (role, color), amount, opacity)
                    .and_then(|brush| xaml.handle_of(brush));
                match made {
                    Some(brush) => {
                        self.set(
                            xaml,
                            found,
                            index,
                            brush,
                            &value.to_string(),
                            &write.restore,
                        );
                    }
                    None => log!("{label} {handle:#x}: no blur brush"),
                }
            }
            Make::Xaml(document) => {
                // Held until set: the element keeps its own reference.
                let Some(object) = markup::load(document, xaml.framework) else {
                    return;
                };
                match xaml.handle_of(&object) {
                    Some(made) => {
                        self.set(xaml, found, index, made, &value.to_string(), &write.restore);
                    }
                    None => log!("{label} {handle:#x}: no handle for the XAML value"),
                }
            }
        }
    }

    /// Puts a new transform by `x`, `y` where ours is, keeping the original
    /// recorded when ours was first set (a measured value that changed).
    pub(super) fn replace_object(&mut self, xaml: &Xaml, found: Match, index: u32, x: f32, y: f32) {
        let Some(target) = self.targets.get(&found) else {
            return;
        };
        let label = target.label;
        let made = translate_document(x, y)
            .and_then(|document| markup::load(&document, xaml.framework))
            .and_then(|object| xaml.handle_of(&object));
        let Some(made) = made else {
            log!("{label} {:#x}: no transform made", found.handle);
            return;
        };
        let hr = xaml.set_property(found.handle, index, made);
        log!("{label} {:#x}: now translate {x},{y}: {hr}", found.handle);
        if hr.is_ok()
            && let Some(applied) = self
                .targets
                .get_mut(&found)
                .and_then(|t| t.applied.as_mut())
        {
            applied.ours = made;
        }
    }

    /// Writes an attached property's plain value (`attached`).
    fn apply_attached(&mut self, xaml: &Xaml, found: Match) {
        let Some(target) = self.targets.get_mut(&found) else {
            return;
        };
        let Value::Text(text) = target.value else {
            log!(
                "{} {:#x}: {} takes plain values only",
                target.label,
                found.handle,
                target.property
            );
            return;
        };
        match attached::write(xaml, found.handle, target.property, text) {
            Ok(written) => {
                log!(
                    "{} {:#x}: {} = {text} (attached)",
                    target.label,
                    found.handle,
                    target.property
                );
                target.applied = Some(Applied {
                    slot: Slot::Attached(written),
                    restore: Restore::Clear,
                    held: None,
                    type_name: None,
                    ours: 0,
                    size: None,
                    rewrites: Rewrites::default(),
                });
            }
            Err(why) => log!(
                "{} {:#x}: {} not set: {why}",
                target.label,
                found.handle,
                target.property
            ),
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
                slot: Slot::Chain(index),
                restore: restore.clone(),
                held,
                type_name: None,
                ours: value,
                size: None,
                rewrites: Rewrites::default(),
            });
        }
        hr.is_ok()
    }
}
