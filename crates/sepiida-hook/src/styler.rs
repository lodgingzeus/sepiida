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
use windows::{
    Win32::System::Com::CoTaskMemFree,
    core::{BSTR, HRESULT, IUnknown, Interface, PCWSTR, w},
};

use crate::{
    log::log,
    xamlom::{
        ISolidColorBrush, IVisualTreeService3, IXamlDiagnostics, InstanceHandle,
        METADATA_IS_VALUE_NULL, PropertyChainSource, PropertyChainValue, bstr_arg,
    },
};

const BRUSH_TYPE: &str = "Windows.UI.Xaml.Media.SolidColorBrush";
const COLOR_TYPE: &str = "Windows.UI.Color";

struct Target {
    fill_index: u32,
    /// The brush the rectangle had before we changed it; `None` if it had
    /// none.
    original: Option<InstanceHandle>,
    styled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorPath {
    Diagnostics,
    Interface,
}

pub struct Styler {
    diagnostics: IXamlDiagnostics,
    service: IVisualTreeService3,
    color: Option<Color>,
    brush: Option<InstanceHandle>,
    color_path: Option<ColorPath>,
    targets: HashMap<Handle, Target>,
}

impl Styler {
    pub fn new(diagnostics: IXamlDiagnostics, service: IVisualTreeService3) -> Self {
        Self {
            diagnostics,
            service,
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
        self.log_chain(handle, "Fill");
        let Some(fill_index) = self.property_index(handle, w!("Fill")) else {
            return;
        };
        let original = self.property(handle, fill_index);
        log!("Fill index {fill_index}, original value {original:#x?}");
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
        for (&handle, target) in &mut self.targets {
            if !target.styled {
                continue;
            }
            // SAFETY: plain calls on a live service with handles it issued.
            let hr = unsafe {
                match target.original {
                    Some(original) => self
                        .service
                        .SetProperty(handle, original, target.fill_index),
                    None => self.service.ClearProperty(handle, target.fill_index),
                }
            };
            log!("restore {handle:#x}: {hr}");
            if hr.is_ok() {
                target.styled = false;
            }
        }
    }

    fn paint(&mut self, handle: Handle) {
        let Some(brush) = self.brush else {
            return;
        };
        let Some(target) = self.targets.get_mut(&handle) else {
            return;
        };
        // SAFETY: plain call on a live service with handles it issued.
        let hr = unsafe { self.service.SetProperty(handle, brush, target.fill_index) };
        log!("set Fill on {handle:#x}: {hr}");
        if hr.is_err() {
            return;
        }
        target.styled = true;
        let fill_index = target.fill_index;
        // Read back: did XAML keep our brush?
        let now = self.property(handle, fill_index);
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
            None => match self.create(BRUSH_TYPE, None) {
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
            self.log_chain(brush, "Color");
        }
        ok
    }

    fn color_by_diagnostics(&self, brush: InstanceHandle, color: Color) -> bool {
        let Some(index) = self.property_index(brush, w!("Color")) else {
            return false;
        };
        let Some(value) = self.create(COLOR_TYPE, Some(&xaml_color(color))) else {
            return false;
        };
        // SAFETY: plain call on a live service with handles it issued.
        let hr = unsafe { self.service.SetProperty(brush, value, index) };
        log_failure("SetProperty(Color)", hr)
    }

    fn color_by_interface(&self, brush: InstanceHandle, color: Color) -> bool {
        let mut raw = core::ptr::null_mut();
        // SAFETY: `raw` receives an owned reference, wrapped right after.
        let hr = unsafe { self.diagnostics.GetIInspectableFromHandle(brush, &mut raw) };
        if !log_failure("GetIInspectableFromHandle", hr) || raw.is_null() {
            return false;
        }
        // SAFETY: a non-null, owned interface pointer from XAML.
        let object = unsafe { IUnknown::from_raw(raw) };
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

    fn create(&self, type_name: &str, value: Option<&str>) -> Option<InstanceHandle> {
        let type_name_b = BSTR::from(type_name);
        let value_b = value.map(BSTR::from).unwrap_or_default();
        let mut handle = 0;
        // SAFETY: the strings outlive the call; `handle` is writable.
        let hr = unsafe {
            self.service
                .CreateInstance(bstr_arg(&type_name_b), bstr_arg(&value_b), &mut handle)
        };
        if hr.is_err() || handle == 0 {
            log!("CreateInstance({type_name}, {value:?}) failed: {hr}");
            return None;
        }
        Some(handle)
    }

    fn property_index(&self, handle: InstanceHandle, name: PCWSTR) -> Option<u32> {
        let mut index = 0;
        // SAFETY: `name` is a static string; `index` is writable.
        let hr = unsafe { self.service.GetPropertyIndex(handle, name, &mut index) };
        if hr.is_err() {
            // SAFETY: `w!` strings are valid and NUL-terminated.
            let name = unsafe { name.to_string() }.unwrap_or_default();
            log!("GetPropertyIndex({handle:#x}, {name}) failed: {hr}");
            return None;
        }
        Some(index)
    }

    fn property(&self, handle: InstanceHandle, index: u32) -> Option<InstanceHandle> {
        let mut value = 0;
        // SAFETY: `value` is writable.
        let hr = unsafe { self.service.GetProperty(handle, index, &mut value) };
        if hr.is_err() {
            log!("GetProperty({handle:#x}, {index}) failed: {hr}");
        }
        (hr.is_ok() && value != 0).then_some(value)
    }

    /// Logs one property's entries in the element's property chain: its
    /// type and value strings and where the value comes from (style,
    /// template, local value…). This is how XAML itself names the types
    /// and formats the values we pass to `CreateInstance`.
    fn log_chain(&self, handle: InstanceHandle, property: &str) {
        let (mut source_count, mut value_count) = (0u32, 0u32);
        let mut sources: *mut PropertyChainSource = core::ptr::null_mut();
        let mut values: *mut PropertyChainValue = core::ptr::null_mut();
        // SAFETY: all out pointers are writable.
        let hr = unsafe {
            self.service.GetPropertyValuesChain(
                handle,
                &mut source_count,
                &mut sources,
                &mut value_count,
                &mut values,
            )
        };
        if hr.is_err() {
            log!("GetPropertyValuesChain({handle:#x}) failed: {hr}");
        }
        // SAFETY: on success XAML returns arrays of the given lengths,
        // allocated with CoTaskMemAlloc, whose strings we now own.
        let (sources, values) = unsafe {
            (
                take_array(sources, source_count),
                take_array(values, value_count),
            )
        };
        for value in values.iter().filter(|v| v.PropertyName == property) {
            let source = sources
                .get(value.PropertyChainIndex as usize)
                .map(|s| (s.Source.0, s.Name.to_string(), s.TargetType.to_string()));
            log!(
                "{property} of {handle:#x}: type={} declared={} value_type={} value={} overridden={} null={} source={source:?}",
                value.Type,
                value.DeclaringType,
                value.ValueType,
                value.Value,
                value.Overridden.as_bool(),
                value.MetadataBits & METADATA_IS_VALUE_NULL != 0,
            );
        }
    }
}

fn log_failure(what: &str, hr: HRESULT) -> bool {
    if hr.is_err() {
        log!("{what} failed: {hr}");
    }
    hr.is_ok()
}

/// Moves the elements out of a COM-allocated array and frees the array.
///
/// # Safety
/// `array` is null or points to `count` initialised elements allocated with
/// `CoTaskMemAlloc` that the caller owns.
unsafe fn take_array<T>(array: *mut T, count: u32) -> Vec<T> {
    if array.is_null() {
        return Vec::new();
    }
    let items = (0..count as usize)
        // SAFETY: in bounds per the contract; each element is read once.
        .map(|i| unsafe { array.add(i).read() })
        .collect();
    // SAFETY: allocated with CoTaskMemAlloc per the contract.
    unsafe { CoTaskMemFree(Some(array.cast_const().cast())) };
    items
}
