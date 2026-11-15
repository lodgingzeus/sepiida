//! Puts the originals back.

use sepiida_core::shell::{finder::Match, layout::Restore};

use super::state::{Layouts, Slot};
use crate::{log::log, properties::Xaml, styler::attached};

impl Layouts {
    pub(super) fn restore(&mut self, xaml: &Xaml, found: Match) {
        let Some(target) = self.targets.get_mut(&found) else {
            return;
        };
        // A measured value is worked out afresh next time.
        target.ready = false;
        target.held_back = false;
        let Some(applied) = target.applied.take() else {
            return;
        };
        let (handle, restore) = (found.handle, &applied.restore);
        let index = match &applied.slot {
            Slot::Chain(index) => *index,
            Slot::Attached(written) => {
                let hr = attached::restore(xaml, handle, written);
                log!("{} {handle:#x}: restored (attached): {hr}", target.label);
                if hr.is_err() {
                    target.applied = Some(applied);
                }
                return;
            }
        };
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

    /// Puts `found`'s original back and forgets the target. False (target
    /// kept) if the original could not be put back.
    pub(super) fn drop_target(&mut self, xaml: &Xaml, found: Match) -> bool {
        self.restore(xaml, found);
        if self
            .targets
            .get(&found)
            .is_some_and(|t| t.applied.is_some())
        {
            return false;
        }
        self.targets.remove(&found);
        true
    }

    /// Puts every original back and turns the look off. The colours are
    /// forgotten; the brushes are kept for the next time.
    pub fn restore_all(&mut self, xaml: &Xaml) {
        self.colors.clear();
        if self.on || self.targets.values().any(|t| t.applied.is_some()) {
            self.set_on(xaml, false);
        }
        self.restore_thinned(xaml);
    }
}
