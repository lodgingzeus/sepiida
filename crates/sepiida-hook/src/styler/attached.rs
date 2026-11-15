//! Attached properties (`Grid.Row`, `Canvas.ZIndex`) on system XAML
//! elements, the way the Windhawk stylers set every property: a `Style`
//! with one `Setter` is loaded with `XamlReader`, which resolves the
//! property and parses the value to its type; the element then gets it
//! through `DependencyObject.SetValue`. The element's local value is read
//! first and put back on restore (cleared if it had none).

use core::ffi::c_void;

use sepiida_core::shell::{framework::Framework, markup::setter_document};
use windows::{
    Win32::System::WinRT::RoGetActivationFactory,
    core::{HRESULT, HSTRING, IUnknown, Interface},
};

use super::markup;
use crate::{
    properties::Xaml,
    xamlom::{
        IDependencyObject, IDependencyPropertyStatics, ISetter, ISetterBaseVector, IStyle,
        InstanceHandle,
    },
};

/// Our value on an element, and what to put back.
pub struct Written {
    property: IUnknown,
    /// The element's local value before ours; `None` if it had none.
    original: Option<IUnknown>,
}

/// Sets the attached `property` of the element behind `handle` to the
/// plain `value`. Returns what to restore, or why it was not set.
pub fn write(
    xaml: &Xaml,
    handle: InstanceHandle,
    property: &str,
    value: &str,
) -> Result<Written, String> {
    if xaml.framework != Framework::System {
        return Err("attached properties need system XAML".to_owned());
    }
    let document = setter_document(property, value).ok_or("not a plain attached value")?;
    let (resolved, parsed) = resolve(&document)?;
    let element = element(xaml, handle)?;
    // SAFETY: a live element and property; `out` receives an owned
    // reference.
    let local = read(|out| unsafe { element.ReadLocalValue(resolved.as_raw(), out) })
        .ok_or("ReadLocalValue failed")?;
    let original = local.filter(|v| !is_unset(v));
    // SAFETY: a live element, property and value; XAML adds its own
    // references.
    let hr = unsafe { element.SetValue(resolved.as_raw(), parsed.as_raw()) };
    if hr.is_err() {
        return Err(format!("SetValue: {hr}"));
    }
    Ok(Written {
        property: resolved,
        original,
    })
}

/// Puts the element's original back: its old local value, or none.
pub fn restore(xaml: &Xaml, handle: InstanceHandle, written: &Written) -> HRESULT {
    let element = match element(xaml, handle) {
        Ok(element) => element,
        Err(_) => return windows::Win32::Foundation::E_FAIL,
    };
    // SAFETY: as in `write`.
    unsafe {
        match &written.original {
            Some(value) => element.SetValue(written.property.as_raw(), value.as_raw()),
            None => element.ClearValue(written.property.as_raw()),
        }
    }
}

pub(super) fn element(xaml: &Xaml, handle: InstanceHandle) -> Result<IDependencyObject, String> {
    xaml.object(handle)
        .ok_or("no object for the handle")?
        .cast()
        .map_err(|e| format!("not a DependencyObject: {e}"))
}

/// The property and the parsed value of the document's only setter, or
/// the step that failed.
pub(super) fn resolve(document: &str) -> Result<(IUnknown, IUnknown), &'static str> {
    let style: IStyle = markup::load(document, Framework::System)
        .ok_or("XamlReader refused the Style")?
        .cast()
        .map_err(|_| "the document is not a Style")?;
    // SAFETY (each call): live objects; each out pointer receives an
    // owned reference or stays null.
    let setters: ISetterBaseVector = read(|out| unsafe { style.get_Setters(out) })
        .flatten()
        .ok_or("Style.Setters failed")?
        .cast()
        .map_err(|_| "Setters is not IVector<SetterBase>")?;
    let setter: ISetter = read(|out| unsafe { setters.GetAt(0, out) })
        .flatten()
        .ok_or("Setters.GetAt(0) failed")?
        .cast()
        .map_err(|_| "the setter is not a Setter")?;
    let property = read(|out| unsafe { setter.get_Property(out) })
        .flatten()
        .ok_or("Setter.Property failed")?;
    let value = read(|out| unsafe { setter.get_Value(out) })
        .flatten()
        .ok_or("Setter.Value failed")?;
    Ok((property, value))
}

/// Calls a WinRT getter: `None` if it failed, `Some(None)` for a null
/// result.
pub(super) fn read(call: impl FnOnce(*mut *mut c_void) -> HRESULT) -> Option<Option<IUnknown>> {
    let mut raw: *mut c_void = core::ptr::null_mut();
    if call(&mut raw).is_err() {
        return None;
    }
    // SAFETY: a non-null result is an owned interface pointer.
    Some((!raw.is_null()).then(|| unsafe { IUnknown::from_raw(raw) }))
}

/// Whether `value` is `DependencyProperty.UnsetValue` (no local value).
pub(super) fn is_unset(value: &IUnknown) -> bool {
    let class = HSTRING::from("Windows.UI.Xaml.DependencyProperty");
    // SAFETY: a WinRT call with a valid class name, on a XAML thread.
    let Ok(statics) = (unsafe { RoGetActivationFactory::<IDependencyPropertyStatics>(&class) })
    else {
        return false;
    };
    // SAFETY: as in `resolve`.
    let Some(Some(unset)) = read(|out| unsafe { statics.get_UnsetValue(out) }) else {
        return false;
    };
    // Identity: compare the canonical IUnknown of both.
    matches!(
        (value.cast::<IUnknown>(), unset.cast::<IUnknown>()),
        (Ok(a), Ok(b)) if a == b
    )
}
