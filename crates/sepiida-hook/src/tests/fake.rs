//! A fake XAML Diagnostics service that records every call the DLL makes
//! and behaves like Explorer where that matters (stale handles, a failing
//! `GetPropertyIndex`).

use core::ffi::c_void;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, E_NOTIMPL, ERROR_NOT_FOUND, RECT, S_OK},
        System::Com::CoTaskMemAlloc,
    },
    core::{BOOL, BSTR, HRESULT, IUnknown, Interface, PCWSTR, implement},
};

use crate::{tap::Factory, xamlom::*};

pub(super) const FILL_INDEX: u32 = 7;
pub(super) const COLOR_INDEX: u32 = 9;
pub(super) const ORIGINAL_FILL: InstanceHandle = 0x100;
/// The handle the original brush gets once its first handle went stale.
pub(super) const LIVE_FILL: InstanceHandle = 0x200;
pub(super) const RECT_HANDLE: InstanceHandle = 5;

pub(super) struct SendCallback(pub(super) IVisualTreeServiceCallback);
// SAFETY: test-only; the callback is only called from the test thread.
unsafe impl Send for SendCallback {}

#[derive(Default)]
pub(super) struct Shared {
    pub(super) calls: Mutex<Vec<String>>,
    pub(super) callback: Mutex<Option<SendCallback>>,
    pub(super) values: Mutex<HashMap<(InstanceHandle, u32), InstanceHandle>>,
    pub(super) next_handle: AtomicU64,
}

impl Shared {
    pub(super) fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub(super) fn has(&self, call: &str) -> bool {
        self.calls().iter().any(|c| c == call)
    }

    pub(super) fn count(&self, call: &str) -> usize {
        self.calls().iter().filter(|c| *c == call).count()
    }
}

#[implement(IXamlDiagnostics, IVisualTreeService3)]
pub(super) struct FakeXaml(Arc<Shared>);

pub(super) fn text(p: *const u16) -> String {
    if p.is_null() {
        return "null".into();
    }
    // SAFETY: the DLL passes NUL-terminated strings.
    unsafe { PCWSTR(p).to_string() }.unwrap_or_default()
}

impl IVisualTreeService_Impl for FakeXaml_Impl {
    unsafe fn AdviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT {
        // SAFETY: the DLL passes a live callback pointer.
        let callback = unsafe { IVisualTreeServiceCallback::from_raw_borrowed(&callback) };
        self.0.record("Advise".into());
        *self.0.callback.lock().unwrap() = callback.cloned().map(SendCallback);
        S_OK
    }
    unsafe fn UnadviseVisualTreeChange(&self, _callback: *mut c_void) -> HRESULT {
        self.0.record("Unadvise".into());
        S_OK
    }
    unsafe fn GetEnums(&self, _: *mut u32, _: *mut *mut c_void) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn CreateInstance(
        &self,
        type_name: *const u16,
        value: *const u16,
        instance: *mut InstanceHandle,
    ) -> HRESULT {
        let handle = 0x1000 + self.0.next_handle.fetch_add(1, Ordering::SeqCst);
        self.0.record(format!(
            "CreateInstance {} {} -> {handle:#x}",
            text(type_name),
            text(value)
        ));
        // SAFETY: the DLL passes a writable pointer.
        unsafe { *instance = handle };
        S_OK
    }
    unsafe fn GetPropertyValuesChain(
        &self,
        instance: InstanceHandle,
        source_count: *mut u32,
        sources: *mut *mut PropertyChainSource,
        property_count: *mut u32,
        values: *mut *mut PropertyChainValue,
    ) -> HRESULT {
        self.0
            .record(format!("GetPropertyValuesChain {instance:#x}"));
        // SAFETY: CoTaskMemAlloc'd arrays handed to the caller like XAML
        // does; the caller frees them. Like Explorer's chain, it lists
        // each property with the index `SetProperty` takes.
        unsafe {
            let source: *mut PropertyChainSource =
                CoTaskMemAlloc(size_of::<PropertyChainSource>()).cast();
            source.write(PropertyChainSource {
                Handle: instance,
                TargetType: BSTR::from("Target"),
                Name: BSTR::from("Style"),
                Source: BaseValueSource(3),
                SrcInfo: empty_source_info(),
            });
            let chain = [
                chain_value(FILL_INDEX, "Fill", "Windows.UI.Xaml.Media.Brush"),
                chain_value(COLOR_INDEX, "Color", "Windows.UI.Color"),
            ];
            let value: *mut PropertyChainValue =
                CoTaskMemAlloc(size_of::<PropertyChainValue>() * chain.len()).cast();
            for (i, entry) in chain.into_iter().enumerate() {
                value.add(i).write(entry);
            }
            *sources = source;
            *source_count = 1;
            *values = value;
            *property_count = 2;
        }
        S_OK
    }
    unsafe fn SetProperty(
        &self,
        instance: InstanceHandle,
        value: InstanceHandle,
        index: u32,
    ) -> HRESULT {
        self.0
            .record(format!("SetProperty {instance:#x} {index} {value:#x}"));
        // Like Explorer: once replaced, the original brush's first handle
        // is stale.
        if value == ORIGINAL_FILL {
            return ERROR_NOT_FOUND.to_hresult();
        }
        self.0
            .values
            .lock()
            .unwrap()
            .insert((instance, index), value);
        S_OK
    }
    unsafe fn ClearProperty(&self, instance: InstanceHandle, index: u32) -> HRESULT {
        self.0
            .record(format!("ClearProperty {instance:#x} {index}"));
        S_OK
    }
    unsafe fn GetCollectionCount(&self, _: InstanceHandle, _: *mut u32) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetCollectionElements(
        &self,
        _: InstanceHandle,
        _: u32,
        _: *mut u32,
        _: *mut *mut c_void,
    ) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn AddChild(&self, _: InstanceHandle, _: InstanceHandle, _: u32) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn RemoveChild(&self, _: InstanceHandle, _: u32) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn ClearChildren(&self, _: InstanceHandle) -> HRESULT {
        E_NOTIMPL
    }
}

impl IVisualTreeService2_Impl for FakeXaml_Impl {
    unsafe fn GetPropertyIndex(
        &self,
        object: InstanceHandle,
        name: PCWSTR,
        _index: *mut u32,
    ) -> HRESULT {
        let name = text(name.0);
        self.0
            .record(format!("GetPropertyIndex {object:#x} {name}"));
        // Explorer on Windows 11 24H2 answers E_INVALIDARG for every name.
        E_INVALIDARG
    }
    unsafe fn GetProperty(
        &self,
        object: InstanceHandle,
        index: u32,
        value: *mut InstanceHandle,
    ) -> HRESULT {
        let current = self
            .0
            .values
            .lock()
            .unwrap()
            .get(&(object, index))
            .copied()
            .unwrap_or(0);
        // SAFETY: the DLL passes a writable pointer.
        unsafe { *value = current };
        S_OK
    }
    unsafe fn ReplaceResource(
        &self,
        _: InstanceHandle,
        _: InstanceHandle,
        _: InstanceHandle,
    ) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn RenderTargetBitmap(
        &self,
        _: InstanceHandle,
        _: i32,
        _: u32,
        _: u32,
        _: *mut *mut c_void,
    ) -> HRESULT {
        E_NOTIMPL
    }
}

impl IVisualTreeService3_Impl for FakeXaml_Impl {
    unsafe fn ResolveResource(&self, _: InstanceHandle, _: PCWSTR, _: i32, _: u32) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetDictionaryItem(
        &self,
        _: InstanceHandle,
        _: PCWSTR,
        _: BOOL,
        _: *mut InstanceHandle,
    ) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn AddDictionaryItem(
        &self,
        _: InstanceHandle,
        _: InstanceHandle,
        _: InstanceHandle,
    ) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn RemoveDictionaryItem(&self, _: InstanceHandle, _: InstanceHandle) -> HRESULT {
        E_NOTIMPL
    }
}

impl IXamlDiagnostics_Impl for FakeXaml_Impl {
    unsafe fn GetDispatcher(&self, _: *mut *mut c_void) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetUiLayer(&self, _: *mut *mut c_void) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetApplication(&self, _: *mut *mut c_void) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetIInspectableFromHandle(
        &self,
        handle: InstanceHandle,
        object: *mut *mut c_void,
    ) -> HRESULT {
        // Explorer's own brushes; anything else stands for an object the
        // fake does not model.
        if handle != ORIGINAL_FILL && handle != LIVE_FILL {
            return E_NOTIMPL;
        }
        // Any COM object stands in for the original brush.
        let brush: IUnknown = Factory.into();
        // SAFETY: the DLL passes a writable pointer and takes ownership.
        unsafe { *object = brush.into_raw() };
        S_OK
    }
    unsafe fn GetHandleFromIInspectable(
        &self,
        _: *mut c_void,
        handle: *mut InstanceHandle,
    ) -> HRESULT {
        self.0.record("GetHandleFromIInspectable".into());
        // SAFETY: the DLL passes a writable pointer.
        unsafe { *handle = LIVE_FILL };
        S_OK
    }
    unsafe fn HitTest(&self, _: RECT, _: *mut u32, _: *mut *mut InstanceHandle) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn RegisterInstance(&self, _: *mut c_void, _: *mut InstanceHandle) -> HRESULT {
        E_NOTIMPL
    }
    unsafe fn GetInitializationData(&self, _: *mut BSTR) -> HRESULT {
        E_NOTIMPL
    }
}

pub(super) fn chain_value(index: u32, name: &str, type_name: &str) -> PropertyChainValue {
    PropertyChainValue {
        Index: index,
        Type: BSTR::from(type_name),
        DeclaringType: BSTR::from("Declaring"),
        ValueType: BSTR::from(type_name),
        ItemType: BSTR::new(),
        Value: BSTR::from("value"),
        Overridden: BOOL(0),
        MetadataBits: 0,
        PropertyName: BSTR::from(name),
        PropertyChainIndex: 0,
    }
}

pub(super) fn empty_source_info() -> SourceInfo {
    SourceInfo {
        FileName: BSTR::new(),
        LineNumber: 0,
        ColumnNumber: 0,
        CharPosition: 0,
        Hash: BSTR::new(),
    }
}

pub(super) fn new_fake() -> (Arc<Shared>, IUnknown) {
    let shared = Arc::new(Shared::default());
    shared
        .values
        .lock()
        .unwrap()
        .insert((RECT_HANDLE, FILL_INDEX), ORIGINAL_FILL);
    let site: IUnknown = FakeXaml(shared.clone()).into();
    (shared, site)
}
