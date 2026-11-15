//! The transparent brush the look puts on layers that would cover a
//! surface's glass (`Value::Transparent`). Made once, on first use, and
//! held for the life of the styler so its handle never goes stale; one
//! brush on many elements is fine in XAML.

use windows::core::{IUnknown, Interface};

use crate::{
    log::log,
    properties::{Xaml, log_failure},
    xamlom::{ISolidColorBrush, InstanceHandle},
};

/// Below the framework's root namespace.
const BRUSH_TYPE: &str = "Media.SolidColorBrush";
const COLOR_TYPE: &str = "Windows.UI.Color";
/// Fully transparent black, as XAML writes `Transparent`.
const TRANSPARENT: &str = "#00000000";

#[derive(Default)]
pub struct TransparentBrush {
    object: Option<IUnknown>,
}

impl TransparentBrush {
    /// A handle, valid now, to the brush; made the first time. `None` if
    /// XAML would not make it or give it the transparent colour.
    pub fn handle(&mut self, xaml: &Xaml) -> Option<InstanceHandle> {
        if self.object.is_none() {
            self.object = make(xaml);
        }
        xaml.handle_of(self.object.as_ref()?)
    }
}

fn make(xaml: &Xaml) -> Option<IUnknown> {
    let handle = xaml.create(&xaml.type_name(BRUSH_TYPE), None)?;
    let object = xaml.object(handle)?;
    let ok = by_diagnostics(xaml, handle) || by_interface(&object);
    log!("transparent brush {handle:#x}: {ok}");
    ok.then_some(object)
}

fn by_diagnostics(xaml: &Xaml, brush: InstanceHandle) -> bool {
    let Some(index) = xaml.property_index(brush, "Color") else {
        return false;
    };
    let Some(value) = xaml.create(COLOR_TYPE, Some(TRANSPARENT)) else {
        return false;
    };
    log_failure("SetProperty(Color)", xaml.set_property(brush, index, value))
}

fn by_interface(object: &IUnknown) -> bool {
    let brush: ISolidColorBrush = match object.cast() {
        Ok(brush) => brush,
        Err(error) => {
            log!("brush is not an ISolidColorBrush: {error}");
            return false;
        }
    };
    let clear = windows::UI::Color {
        A: 0,
        R: 0,
        G: 0,
        B: 0,
    };
    // SAFETY: plain call on a live interface.
    let hr = unsafe { brush.put_Color(clear) };
    log_failure("ISolidColorBrush::put_Color", hr)
}
