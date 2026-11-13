//! Reading and writing element properties through XAML Diagnostics: property
//! indices, values, new instances and the objects behind handles. Every
//! failure is logged here, since each Windows round trip is expensive.

use sepiida_core::shell::tree::Handle;
use windows::{
    Win32::System::Com::CoTaskMemFree,
    core::{BSTR, HRESULT, HSTRING, IUnknown, Interface, PCWSTR},
};

use crate::{
    log::log,
    xamlom::{
        IVisualTreeService3, IXamlDiagnostics, InstanceHandle, METADATA_IS_VALUE_NULL,
        PropertyChainSource, PropertyChainValue, bstr_arg,
    },
};

/// The two diagnostics interfaces XAML hands us, used together.
pub struct Xaml {
    pub diagnostics: IXamlDiagnostics,
    pub service: IVisualTreeService3,
}

impl Xaml {
    pub fn new(diagnostics: IXamlDiagnostics, service: IVisualTreeService3) -> Self {
        Self {
            diagnostics,
            service,
        }
    }

    /// The object behind `handle`, as an owned reference.
    pub fn object(&self, handle: InstanceHandle) -> Option<IUnknown> {
        let mut raw = core::ptr::null_mut();
        // SAFETY: `raw` receives an owned reference, wrapped right after.
        let hr = unsafe { self.diagnostics.GetIInspectableFromHandle(handle, &mut raw) };
        if !log_failure("GetIInspectableFromHandle", hr) || raw.is_null() {
            return None;
        }
        // SAFETY: a non-null, owned interface pointer from XAML.
        Some(unsafe { IUnknown::from_raw(raw) })
    }

    /// A handle for `object` that is valid now. XAML Diagnostics holds
    /// handles weakly, so one recorded earlier may have gone stale.
    pub fn handle_of(&self, object: &IUnknown) -> Option<InstanceHandle> {
        let mut handle = 0;
        // SAFETY: a live object pointer, borrowed for the call; `handle`
        // is writable.
        let hr = unsafe {
            self.diagnostics
                .GetHandleFromIInspectable(object.as_raw(), &mut handle)
        };
        if hr.is_ok() && handle != 0 {
            return Some(handle);
        }
        log!("GetHandleFromIInspectable failed: {hr}; registering the object");
        // SAFETY: as above.
        let hr = unsafe {
            self.diagnostics
                .RegisterInstance(object.as_raw(), &mut handle)
        };
        if hr.is_ok() && handle != 0 {
            return Some(handle);
        }
        log!("RegisterInstance failed: {hr}");
        None
    }

    pub fn create(&self, type_name: &str, value: Option<&str>) -> Option<InstanceHandle> {
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

    /// The index `SetProperty` takes for `property` on `handle`.
    ///
    /// Microsoft documents the property chain as the way to find it.
    /// `GetPropertyIndex` answers E_INVALIDARG in Explorer on Windows 11
    /// 24H2, so it is only a fallback.
    pub fn property_index(&self, handle: InstanceHandle, property: &str) -> Option<u32> {
        self.chain_index(handle, property)
            .or_else(|| self.lookup_index(handle, property))
    }

    fn lookup_index(&self, handle: InstanceHandle, property: &str) -> Option<u32> {
        let name = HSTRING::from(property);
        let mut index = 0;
        // SAFETY: `name` outlives the call and is NUL-terminated; `index`
        // is writable.
        let hr = unsafe {
            self.service
                .GetPropertyIndex(handle, PCWSTR(name.as_ptr()), &mut index)
        };
        if hr.is_err() {
            log!("GetPropertyIndex({handle:#x}, {property}) failed: {hr}");
            return None;
        }
        Some(index)
    }

    /// The current value of a property, or `None` if it has none.
    pub fn property(&self, handle: Handle, index: u32) -> Option<InstanceHandle> {
        let mut value = 0;
        // SAFETY: `value` is writable.
        let hr = unsafe { self.service.GetProperty(handle, index, &mut value) };
        if hr.is_err() {
            log!("GetProperty({handle:#x}, {index}) failed: {hr}");
        }
        (hr.is_ok() && value != 0).then_some(value)
    }

    pub fn set_property(&self, handle: Handle, index: u32, value: InstanceHandle) -> HRESULT {
        // SAFETY: plain call on a live service with handles it issued.
        unsafe { self.service.SetProperty(handle, value, index) }
    }

    pub fn clear_property(&self, handle: Handle, index: u32) -> HRESULT {
        // SAFETY: plain call on a live service with a handle it issued.
        unsafe { self.service.ClearProperty(handle, index) }
    }

    /// Finds `property` in the element's property chain and returns its
    /// index. Logs each of its entries: type and value strings and where
    /// the value comes from (style, template, local value…), which is how
    /// XAML itself names the types and formats the values we pass to
    /// `CreateInstance`.
    pub fn chain_index(&self, handle: InstanceHandle, property: &str) -> Option<u32> {
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
        let entries: Vec<&PropertyChainValue> = values
            .iter()
            .filter(|v| v.PropertyName == property)
            .collect();
        for value in &entries {
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
        // Every entry of one property carries the same index.
        let index = entries.first().map(|value| value.Index);
        if index.is_none() {
            log!("{property} not in the property chain of {handle:#x}");
        }
        index
    }
}

pub fn log_failure(what: &str, hr: HRESULT) -> bool {
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
