//! The `SolidColorBrush` per role that `Paint::Solid` targets share.
//!
//! Changing a role's colour changes its brush's `Color`. A brush is created
//! only once a target of its role exists, so XAML threads without targets
//! stay untouched. Its colour is set through the diagnostics API
//! (`Windows.UI.Color` created from a string) or, if XAML refuses that,
//! through `ISolidColorBrush::put_Color`; whichever works first is kept.

use std::collections::{HashMap, HashSet};

use sepiida_core::{
    Color,
    shell::{protocol::xaml_color, rules::Role},
};
use windows::core::{IUnknown, Interface};

use crate::{
    log::log,
    properties::{Xaml, log_failure},
    xamlom::{ISolidColorBrush, InstanceHandle},
};

const BRUSH_TYPE: &str = "Windows.UI.Xaml.Media.SolidColorBrush";
const COLOR_TYPE: &str = "Windows.UI.Color";

struct Brush {
    handle: InstanceHandle,
    /// Keeps the brush alive while no element uses it (after a restore, or
    /// once Explorer put its own brush back), so `handle` stays valid.
    _object: Option<IUnknown>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorPath {
    Diagnostics,
    Interface,
}

#[derive(Default)]
pub struct SolidBrushes {
    brushes: HashMap<Role, Brush>,
    /// Every brush handle we created, including replaced ones, so that one
    /// of ours is never taken for Explorer's original.
    ours: HashSet<InstanceHandle>,
    color_path: Option<ColorPath>,
}

impl SolidBrushes {
    /// The role's brush, if it was created.
    pub fn handle(&self, role: Role) -> Option<InstanceHandle> {
        self.brushes.get(&role).map(|b| b.handle)
    }

    /// Whether `handle` is a brush we created.
    pub fn is_ours(&self, handle: InstanceHandle) -> bool {
        self.ours.contains(&handle)
    }

    /// Creates the role's brush if needed and gives it `color`. Returns
    /// whether the brush now has that colour. If an existing brush refuses
    /// the colour (its handle may have gone stale), it is replaced by a new
    /// one, once.
    pub fn ensure(&mut self, xaml: &Xaml, role: Role, color: Color) -> bool {
        if let Some(brush) = self.handle(role) {
            if self.color_brush(xaml, role, brush, color) {
                return true;
            }
            log!("{role:?} brush {brush:#x} refused the colour; creating a new one");
            self.brushes.remove(&role);
        }
        let Some(handle) = xaml.create(BRUSH_TYPE, None) else {
            return false;
        };
        let object = xaml.object(handle);
        self.ours.insert(handle);
        self.brushes.insert(
            role,
            Brush {
                handle,
                _object: object,
            },
        );
        self.color_brush(xaml, role, handle, color)
    }

    fn color_brush(
        &mut self,
        xaml: &Xaml,
        role: Role,
        brush: InstanceHandle,
        color: Color,
    ) -> bool {
        let ok = match self.color_path {
            Some(ColorPath::Diagnostics) => color_by_diagnostics(xaml, brush, color),
            Some(ColorPath::Interface) => color_by_interface(xaml, brush, color),
            None => {
                if color_by_diagnostics(xaml, brush, color) {
                    self.color_path = Some(ColorPath::Diagnostics);
                    true
                } else if color_by_interface(xaml, brush, color) {
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
            xaml.chain_index(brush, "Color");
        }
        ok
    }
}

fn color_by_diagnostics(xaml: &Xaml, brush: InstanceHandle, color: Color) -> bool {
    let Some(index) = xaml.property_index(brush, "Color") else {
        return false;
    };
    let Some(value) = xaml.create(COLOR_TYPE, Some(&xaml_color(color))) else {
        return false;
    };
    log_failure("SetProperty(Color)", xaml.set_property(brush, index, value))
}

fn color_by_interface(xaml: &Xaml, brush: InstanceHandle, color: Color) -> bool {
    let Some(object) = xaml.object(brush) else {
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
