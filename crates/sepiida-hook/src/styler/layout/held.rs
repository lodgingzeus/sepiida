//! Glass under a held acrylic (`sepiida_core::shell::held_glass`): where
//! an animation holds Windows' `AcrylicBrush` on a surface our blur is
//! for, that brush's luminosity is lowered while the glass look is on,
//! and put back when the look goes off or the material turns solid. The
//! brush may be shared, so each is changed once, by its handle.

use sepiida_core::shell::held_glass::{
    HELD_LUMINOSITY, LUMINOSITY_PROPERTY, LUMINOSITY_TYPE, LuminosityRestore, luminosity_restore,
};

use super::state::Layouts;
use crate::{log::log, properties::Xaml, xamlom::InstanceHandle};

/// A brush whose luminosity we lowered.
pub(super) struct Thinned {
    index: u32,
    restore: LuminosityRestore,
}

impl Layouts {
    /// `held` is the held value as XAML formats it: a brush's handle.
    pub(super) fn thin_held(
        &mut self,
        xaml: &Xaml,
        label: &str,
        element: InstanceHandle,
        held: &str,
    ) {
        let Ok(brush) = held.trim().parse::<InstanceHandle>() else {
            log!("{label} {element:#x}: held value {held:?} is not an object; left alone");
            return;
        };
        if brush == 0 || self.thinned.contains_key(&brush) {
            return;
        }
        let Some((index, original)) = xaml.effective(brush, LUMINOSITY_PROPERTY) else {
            log!("{label} {element:#x}: held value {brush:#x} is not an acrylic; left alone");
            return;
        };
        let Some(ours) = xaml.create(LUMINOSITY_TYPE, Some(HELD_LUMINOSITY)) else {
            log!("{label} {element:#x}: luminosity {HELD_LUMINOSITY} not made");
            return;
        };
        let hr = xaml.set_property(brush, index, ours);
        log!(
            "{label} {element:#x}: held brush {brush:#x}: {LUMINOSITY_PROPERTY} {} -> {HELD_LUMINOSITY}: {hr}",
            original.value
        );
        if hr.is_ok() {
            let restore = luminosity_restore(original.source, &original.value);
            self.thinned.insert(brush, Thinned { index, restore });
        }
    }

    /// Puts every lowered luminosity back. One that fails is kept, so a
    /// later call tries again.
    pub(super) fn restore_thinned(&mut self, xaml: &Xaml) {
        let brushes: Vec<InstanceHandle> = self.thinned.keys().copied().collect();
        for brush in brushes {
            let Some(thinned) = self.thinned.get(&brush) else {
                continue;
            };
            let hr = match &thinned.restore {
                LuminosityRestore::Recreate(value) => {
                    match xaml.create(LUMINOSITY_TYPE, Some(value)) {
                        Some(original) => xaml.set_property(brush, thinned.index, original),
                        None => xaml.clear_property(brush, thinned.index),
                    }
                }
                LuminosityRestore::Clear => xaml.clear_property(brush, thinned.index),
            };
            log!(
                "held brush {brush:#x}: {LUMINOSITY_PROPERTY} restored ({:?}): {hr}",
                thinned.restore
            );
            if hr.is_ok() {
                self.thinned.remove(&brush);
            }
        }
    }
}
