//! Recolours the taskbar background rectangles through XAML Diagnostics,
//! and puts the original brushes back.
//!
//! One `SolidColorBrush` is created per attachment and shared by every
//! taskbar; changing colour changes that brush's `Color`. Its colour is set
//! through the diagnostics API (`Windows.UI.Color` created from a string)
//! or, if XAML refuses that, through `ISolidColorBrush::put_Color`. Which
//! path works is not documented, so both are tried and logged.

use std::collections::HashMap;

use sepiida_core::{Color, shell::protocol::xaml_color, shell::tree::Handle};
use windows::core::{IUnknown, Interface};

use crate::{
    log::log,
    properties::{Xaml, log_failure},
    xamlom::{ISolidColorBrush, IVisualTreeService3, IXamlDiagnostics, InstanceHandle},
};

const BRUSH_TYPE: &str = "Windows.UI.Xaml.Media.SolidColorBrush";
const COLOR_TYPE: &str = "Windows.UI.Color";

struct Target {
    fill_index: u32,
    /// The brush the rectangle had before we changed it; `None` if it had
    /// none.
    original: Option<Original>,
    styled: bool,
}

struct Original {
    handle: InstanceHandle,
    /// A strong reference to the brush. XAML Diagnostics holds handles
    /// weakly: once our brush replaces this one nothing else keeps it
    /// alive, and `handle` goes stale (ERROR_NOT_FOUND on restore).
    object: Option<IUnknown>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorPath {
    Diagnostics,
    Interface,
}

pub struct Styler {
    xaml: Xaml,
    color: Option<Color>,
    brush: Option<InstanceHandle>,
    color_path: Option<ColorPath>,
    targets: HashMap<Handle, Target>,
}

impl Styler {
    pub fn new(diagnostics: IXamlDiagnostics, service: IVisualTreeService3) -> Self {
        Self {
            xaml: Xaml::new(diagnostics, service),
            color: None,
            brush: None,
            color_path: None,
            targets: HashMap::new(),
        }
    }

    /// A taskbar background rectangle appeared. Records its original brush
    /// and paints it if a colour is set.
    pub fn found(&mut self, handle: Handle) {
        if self.targets.contains_key(&handle) {
            return;
        }
        log!("taskbar background found: {handle:#x}");
        let Some(fill_index) = self.xaml.property_index(handle, "Fill") else {
            return;
        };
        let original = self
            .xaml
            .property(handle, fill_index)
            .map(|value| Original {
                handle: value,
                object: self.xaml.object(value),
            });
        log!(
            "Fill index {fill_index}, original value {:#x?} (held: {})",
            original.as_ref().map(|o| o.handle),
            original.as_ref().is_some_and(|o| o.object.is_some()),
        );
        self.targets.insert(
            handle,
            Target {
                fill_index,
                original,
                styled: false,
            },
        );
        if self.color.is_some() {
            self.paint(handle);
        }
    }

    /// Elements that left the tree. Nothing to restore on them.
    pub fn forget(&mut self, handles: &[Handle]) {
        for handle in handles {
            if self.targets.remove(handle).is_some() {
                log!("taskbar background removed: {handle:#x}");
            }
        }
    }

    /// Paints every taskbar background in `color`. If the brush cannot take
    /// the colour, nothing is painted and the previous colour (if any)
    /// stays: an uncoloured brush would make the taskbar transparent.
    pub fn set_color(&mut self, color: Color) {
        if !self.update_brush(color) {
            return;
        }
        self.color = Some(color);
        let handles: Vec<Handle> = self.targets.keys().copied().collect();
        for handle in handles {
            self.paint(handle);
        }
    }

    pub fn restore_all(&mut self) {
        self.color = None;
        let styled: Vec<Handle> = self
            .targets
            .iter()
            .filter(|(_, target)| target.styled)
            .map(|(&handle, _)| handle)
            .collect();
        for handle in styled {
            self.restore(handle);
        }
    }

    fn restore(&mut self, handle: Handle) {
        let Some(target) = self.targets.get(&handle) else {
            return;
        };
        let fill_index = target.fill_index;
        let hr = match &target.original {
            Some(original) => {
                let value = self.live_handle(original);
                self.xaml.set_property(handle, fill_index, value)
            }
            None => self.xaml.clear_property(handle, fill_index),
        };
        log!("restore {handle:#x}: {hr}");
        if hr.is_ok()
            && let Some(target) = self.targets.get_mut(&handle)
        {
            target.styled = false;
        }
    }

    /// A handle to the original brush that is valid now: asked for afresh
    /// from the object we hold, since the handle recorded at discovery
    /// may have gone stale.
    fn live_handle(&self, original: &Original) -> InstanceHandle {
        let Some(object) = &original.object else {
            return original.handle;
        };
        self.xaml.handle_of(object).unwrap_or_else(|| {
            log!("trying the recorded handle");
            original.handle
        })
    }

    fn paint(&mut self, handle: Handle) {
        let Some(brush) = self.brush else {
            return;
        };
        let Some(target) = self.targets.get_mut(&handle) else {
            return;
        };
        let hr = self.xaml.set_property(handle, target.fill_index, brush);
        log!("set Fill on {handle:#x}: {hr}");
        if hr.is_err() {
            return;
        }
        target.styled = true;
        let fill_index = target.fill_index;
        // Read back: did XAML keep our brush?
        let now = self.xaml.property(handle, fill_index);
        log!(
            "Fill on {handle:#x} is now {now:#x?} (our brush {brush:#x}): {}",
            if now == Some(brush) {
                "kept"
            } else {
                "NOT kept"
            }
        );
    }

    /// Creates the shared brush if needed and gives it `color`. Returns
    /// whether the brush now has that colour.
    fn update_brush(&mut self, color: Color) -> bool {
        let brush = match self.brush {
            Some(brush) => brush,
            None => match self.xaml.create(BRUSH_TYPE, None) {
                Some(brush) => {
                    self.brush = Some(brush);
                    brush
                }
                None => return false,
            },
        };
        let ok = match self.color_path {
            Some(ColorPath::Diagnostics) => self.color_by_diagnostics(brush, color),
            Some(ColorPath::Interface) => self.color_by_interface(brush, color),
            None => {
                if self.color_by_diagnostics(brush, color) {
                    self.color_path = Some(ColorPath::Diagnostics);
                    true
                } else if self.color_by_interface(brush, color) {
                    self.color_path = Some(ColorPath::Interface);
                    true
                } else {
                    false
                }
            }
        };
        log!("brush colour {color} via {:?}: {ok}", self.color_path);
        if ok {
            // Read back what XAML now reports for the colour.
            self.xaml.chain_index(brush, "Color");
        }
        ok
    }

    fn color_by_diagnostics(&self, brush: InstanceHandle, color: Color) -> bool {
        let Some(index) = self.xaml.property_index(brush, "Color") else {
            return false;
        };
        let Some(value) = self.xaml.create(COLOR_TYPE, Some(&xaml_color(color))) else {
            return false;
        };
        log_failure(
            "SetProperty(Color)",
            self.xaml.set_property(brush, index, value),
        )
    }

    fn color_by_interface(&self, brush: InstanceHandle, color: Color) -> bool {
        let Some(object) = self.xaml.object(brush) else {
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
}
