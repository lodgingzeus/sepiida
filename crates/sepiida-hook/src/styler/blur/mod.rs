//! Blur brushes the look puts on surfaces (`Value::Blur`): the effects
//! (`effects`), the XAML brush that hosts them (`brush`), and one brush per
//! role, amount and opacity, recoloured when the role's colour changes.

mod brush;
mod effects;

use std::collections::HashMap;

use sepiida_core::{Color, shell::rules::Role};
use windows::{
    UI::Composition::{Compositor, Visual},
    Win32::System::WinRT::RoGetActivationFactory,
    core::{HSTRING, IUnknown, Interface},
};

use self::brush::BlurBrush;
use crate::{
    log::log,
    properties::Xaml,
    xamlom::{IElementCompositionPreviewStatics, IID_UI_ELEMENT, InstanceHandle},
};

const PREVIEW_CLASS: &str = "Windows.UI.Xaml.Hosting.ElementCompositionPreview";

/// Brushes are shared by targets with the same role, amount and opacity.
type Key = (Role, u32, u32);

#[derive(Default)]
pub struct BlurBrushes {
    compositor: Option<Compositor>,
    brushes: HashMap<Key, BlurBrush>,
}

impl BlurBrushes {
    /// A brush for `element`'s thread, blurring by `amount` with `color`
    /// at `opacity` over it; made the first time.
    pub fn brush(
        &mut self,
        xaml: &Xaml,
        element: InstanceHandle,
        (role, color): (Role, Color),
        amount: f32,
        opacity: f32,
    ) -> Option<&IUnknown> {
        let key = (role, amount.to_bits(), opacity.to_bits());
        if !self.brushes.contains_key(&key) {
            let compositor = match &self.compositor {
                Some(compositor) => compositor.clone(),
                None => {
                    let compositor = compositor_of(xaml, element)?;
                    self.compositor = Some(compositor.clone());
                    compositor
                }
            };
            let brush = BlurBrush::new(compositor, amount, tint(color, opacity))?;
            log!("blur brush made: {amount} px, {role:?} at {opacity}");
            self.brushes.insert(key, brush);
        }
        self.brushes.get(&key).map(BlurBrush::object)
    }

    /// The role's colour changed: its brushes take it.
    pub fn set_color(&self, role: Role, color: Color) {
        for (&(r, _, opacity), brush) in &self.brushes {
            if r == role {
                brush.set_tint(tint(color, f32::from_bits(opacity)));
            }
        }
    }
}

fn tint(color: Color, opacity: f32) -> windows::UI::Color {
    windows::UI::Color {
        // Validated to 0..=1 in core.
        A: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        R: color.r,
        G: color.g,
        B: color.b,
    }
}

/// The compositor XAML draws `element` with.
fn compositor_of(xaml: &Xaml, element: InstanceHandle) -> Option<Compositor> {
    let object = xaml.object(element)?;
    let result = (|| -> windows::core::Result<Compositor> {
        // SAFETY: a plain WinRT call with a valid class name.
        let preview: IElementCompositionPreviewStatics =
            unsafe { RoGetActivationFactory(&HSTRING::from(PREVIEW_CLASS)) }?;
        let mut ui_element = core::ptr::null_mut();
        // SAFETY: `object` is live; `ui_element` receives an owned
        // reference, wrapped right after.
        unsafe { object.query(&IID_UI_ELEMENT, &mut ui_element) }.ok()?;
        // SAFETY: an owned reference from the query above.
        let ui_element = unsafe { IUnknown::from_raw(ui_element) };
        let mut visual = core::ptr::null_mut();
        // SAFETY: `ui_element` is a live IUIElement; `visual` receives an
        // owned reference.
        unsafe { preview.GetElementVisual(ui_element.as_raw(), &mut visual) }.ok()?;
        // SAFETY: an owned IVisual reference.
        let visual = unsafe { Visual::from_raw(visual) };
        visual.Compositor()
    })();
    match result {
        Ok(compositor) => Some(compositor),
        Err(error) => {
            log!("no compositor for {element:#x}: {error}");
            None
        }
    }
}
