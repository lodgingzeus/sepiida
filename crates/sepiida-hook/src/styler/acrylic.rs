//! Acrylic brushes the look adds (`Value::Acrylic`): one per role, made
//! with `CreateInstance` and tinted through `IAcrylicBrush` like Windows'
//! own (`tint`). Each is held strongly for the life of the styler, so its
//! handle never goes stale; one brush on several elements is fine in XAML.

use std::collections::{HashMap, hash_map::Entry};

use sepiida_core::{
    Color,
    shell::{
        rules::Role,
        tint::{Opacity, own_acrylic_tint},
    },
};
use windows::core::IUnknown;

use super::tint::{Brush, log_details, read_tint, write_tint};
use crate::{log::log, properties::Xaml, xamlom::InstanceHandle};

/// The type `CreateInstance` makes.
/// Below the framework's root namespace.
const ACRYLIC_TYPE: &str = "Media.AcrylicBrush";

struct Own {
    object: IUnknown,
    brush: Brush,
}

#[derive(Default)]
pub struct OwnAcrylic {
    brushes: HashMap<Role, Own>,
}

impl OwnAcrylic {
    /// A handle, valid now, to the role's brush tinted with `color`; the
    /// brush is made the first time. `None` if it could not be made.
    pub fn brush(
        &mut self,
        xaml: &Xaml,
        role: Role,
        color: Color,
        opacity: Option<Opacity>,
    ) -> Option<InstanceHandle> {
        let own = match self.brushes.entry(role) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(make(xaml)?),
        };
        tint(own, role, color, opacity);
        xaml.handle_of(&own.object)
    }

    /// Gives the role's brush, if made, a new colour or opacity.
    pub fn retint(&self, role: Role, color: Color, opacity: Option<Opacity>) {
        if let Some(own) = self.brushes.get(&role) {
            tint(own, role, color, opacity);
        }
    }
}

fn make(xaml: &Xaml) -> Option<Own> {
    let handle = xaml.create(&xaml.type_name(ACRYLIC_TYPE), None)?;
    let object = xaml.object(handle)?;
    let brush = Brush::from_object(&object, handle)?;
    // What a new brush holds before we tint it, and its background source.
    if let Some(current) = read_tint(&brush) {
        log_details(xaml, handle, &brush, current);
    }
    Some(Own { object, brush })
}

fn tint(own: &Own, role: Role, color: Color, opacity: Option<Opacity>) {
    let tint = own_acrylic_tint(color, opacity);
    let ok = write_tint(&own.brush, tint);
    log!(
        "own {role:?} acrylic tinted {} at {}: {ok}",
        tint.color,
        tint.opacity
    );
}
