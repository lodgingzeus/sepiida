//! Hand-written bindings for the XAML Diagnostics interfaces in the SDK's
//! `um/xamlom.idl`, which the `windows` crate does not include. Method order
//! is vtable order and must match the IDL exactly, including methods we
//! never call. Interface parameters are raw pointers and `[in] BSTR`
//! parameters are `*const u16` (the caller keeps ownership); `BSTR` fields
//! in structs we receive are owned by us and free themselves on drop.

#![allow(non_snake_case, reason = "COM method and field names follow the IDL")]

use core::ffi::c_void;

use windows::{
    Win32::Foundation::RECT,
    core::{BOOL, BSTR, GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface},
};

pub type InstanceHandle = u64;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualMutationType(pub i32);

impl VisualMutationType {
    pub const ADD: Self = Self(0);
    pub const REMOVE: Self = Self(1);
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseValueSource(pub i32);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualElementState(pub i32);

#[repr(C)]
#[derive(Debug)]
pub struct SourceInfo {
    pub FileName: BSTR,
    pub LineNumber: u32,
    pub ColumnNumber: u32,
    pub CharPosition: u32,
    pub Hash: BSTR,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ParentChildRelation {
    pub Parent: InstanceHandle,
    pub Child: InstanceHandle,
    pub ChildIndex: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct VisualElement {
    pub Handle: InstanceHandle,
    pub SrcInfo: SourceInfo,
    pub Type: BSTR,
    pub Name: BSTR,
    pub NumChildren: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct PropertyChainSource {
    pub Handle: InstanceHandle,
    pub TargetType: BSTR,
    pub Name: BSTR,
    pub Source: BaseValueSource,
    pub SrcInfo: SourceInfo,
}

#[repr(C)]
#[derive(Debug)]
pub struct PropertyChainValue {
    pub Index: u32,
    pub Type: BSTR,
    pub DeclaringType: BSTR,
    pub ValueType: BSTR,
    pub ItemType: BSTR,
    pub Value: BSTR,
    pub Overridden: BOOL,
    pub MetadataBits: i64,
    pub PropertyName: BSTR,
    pub PropertyChainIndex: u32,
}

/// `MetadataBit::IsValueNull`.
pub const METADATA_IS_VALUE_NULL: i64 = 0x20;

// Layouts as the MSVC x64 compiler lays out the IDL structs.
#[cfg(target_pointer_width = "64")]
const _: () = {
    use core::mem::{align_of, size_of};
    assert!(size_of::<SourceInfo>() == 32);
    assert!(size_of::<ParentChildRelation>() == 24);
    assert!(size_of::<VisualElement>() == 64);
    assert!(size_of::<PropertyChainSource>() == 64);
    assert!(size_of::<PropertyChainValue>() == 80);
    assert!(align_of::<VisualElement>() == 8);
    assert!(size_of::<BSTR>() == size_of::<*const u16>());
};

#[interface("AA7A8931-80E4-4FEC-8F3B-553F87B4966E")]
pub unsafe trait IVisualTreeServiceCallback: IUnknown {
    pub fn OnVisualTreeChange(
        &self,
        relation: ParentChildRelation,
        element: VisualElement,
        mutation_type: VisualMutationType,
    ) -> HRESULT;
}

#[interface("BAD9EB88-AE77-4397-B948-5FA2DB0A19EA")]
pub unsafe trait IVisualTreeServiceCallback2: IVisualTreeServiceCallback {
    pub fn OnElementStateChanged(
        &self,
        element: InstanceHandle,
        element_state: VisualElementState,
        context: PCWSTR,
    ) -> HRESULT;
}

#[interface("A593B11A-D17F-48BB-8F66-83910731C8A5")]
pub unsafe trait IVisualTreeService: IUnknown {
    pub fn AdviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
    pub fn UnadviseVisualTreeChange(&self, callback: *mut c_void) -> HRESULT;
    pub fn GetEnums(&self, count: *mut u32, enums: *mut *mut c_void) -> HRESULT;
    pub fn CreateInstance(
        &self,
        type_name: *const u16,
        value: *const u16,
        instance: *mut InstanceHandle,
    ) -> HRESULT;
    pub fn GetPropertyValuesChain(
        &self,
        instance: InstanceHandle,
        source_count: *mut u32,
        sources: *mut *mut PropertyChainSource,
        property_count: *mut u32,
        values: *mut *mut PropertyChainValue,
    ) -> HRESULT;
    pub fn SetProperty(
        &self,
        instance: InstanceHandle,
        value: InstanceHandle,
        property_index: u32,
    ) -> HRESULT;
    pub fn ClearProperty(&self, instance: InstanceHandle, property_index: u32) -> HRESULT;
    pub fn GetCollectionCount(&self, instance: InstanceHandle, size: *mut u32) -> HRESULT;
    pub fn GetCollectionElements(
        &self,
        instance: InstanceHandle,
        start_index: u32,
        element_count: *mut u32,
        elements: *mut *mut c_void,
    ) -> HRESULT;
    pub fn AddChild(&self, parent: InstanceHandle, child: InstanceHandle, index: u32) -> HRESULT;
    pub fn RemoveChild(&self, parent: InstanceHandle, index: u32) -> HRESULT;
    pub fn ClearChildren(&self, parent: InstanceHandle) -> HRESULT;
}

#[interface("130F5136-EC43-4F61-89C7-9801A36D2E95")]
pub unsafe trait IVisualTreeService2: IVisualTreeService {
    pub fn GetPropertyIndex(
        &self,
        object: InstanceHandle,
        property_name: PCWSTR,
        property_index: *mut u32,
    ) -> HRESULT;
    pub fn GetProperty(
        &self,
        object: InstanceHandle,
        property_index: u32,
        value: *mut InstanceHandle,
    ) -> HRESULT;
    pub fn ReplaceResource(
        &self,
        dictionary: InstanceHandle,
        key: InstanceHandle,
        new_value: InstanceHandle,
    ) -> HRESULT;
    pub fn RenderTargetBitmap(
        &self,
        handle: InstanceHandle,
        options: i32,
        max_pixel_width: u32,
        max_pixel_height: u32,
        bitmap_data: *mut *mut c_void,
    ) -> HRESULT;
}

#[interface("0E79C6E0-85A0-4BE8-B41A-655CF1FD19BD")]
pub unsafe trait IVisualTreeService3: IVisualTreeService2 {
    pub fn ResolveResource(
        &self,
        resource_context: InstanceHandle,
        resource_name: PCWSTR,
        resource_type: i32,
        property_index: u32,
    ) -> HRESULT;
    pub fn GetDictionaryItem(
        &self,
        dictionary: InstanceHandle,
        resource_name: PCWSTR,
        resource_is_implicit_style: BOOL,
        resource: *mut InstanceHandle,
    ) -> HRESULT;
    pub fn AddDictionaryItem(
        &self,
        dictionary: InstanceHandle,
        key: InstanceHandle,
        resource: InstanceHandle,
    ) -> HRESULT;
    pub fn RemoveDictionaryItem(&self, dictionary: InstanceHandle, key: InstanceHandle) -> HRESULT;
}

#[interface("18C9E2B6-3F43-4116-9F2B-FF935D7770D2")]
pub unsafe trait IXamlDiagnostics: IUnknown {
    pub fn GetDispatcher(&self, dispatcher: *mut *mut c_void) -> HRESULT;
    pub fn GetUiLayer(&self, layer: *mut *mut c_void) -> HRESULT;
    pub fn GetApplication(&self, application: *mut *mut c_void) -> HRESULT;
    pub fn GetIInspectableFromHandle(
        &self,
        instance: InstanceHandle,
        inspectable: *mut *mut c_void,
    ) -> HRESULT;
    pub fn GetHandleFromIInspectable(
        &self,
        inspectable: *mut c_void,
        instance: *mut InstanceHandle,
    ) -> HRESULT;
    pub fn HitTest(
        &self,
        rect: RECT,
        count: *mut u32,
        handles: *mut *mut InstanceHandle,
    ) -> HRESULT;
    pub fn RegisterInstance(
        &self,
        inspectable: *mut c_void,
        instance: *mut InstanceHandle,
    ) -> HRESULT;
    pub fn GetInitializationData(&self, data: *mut BSTR) -> HRESULT;
}

/// `Windows.UI.Xaml.Media.ISolidColorBrush`, from the SDK's
/// `winrt/windows.ui.xaml.media.h`. The first three slots after `IUnknown`
/// are `IInspectable`'s, declared here as placeholders.
#[interface("9d850850-66f3-48df-9a8f-824bd5e070af")]
pub unsafe trait ISolidColorBrush: IUnknown {
    pub fn GetIids(&self, count: *mut u32, iids: *mut *mut GUID) -> HRESULT;
    pub fn GetRuntimeClassName(&self, name: *mut *mut c_void) -> HRESULT;
    pub fn GetTrustLevel(&self, level: *mut i32) -> HRESULT;
    pub fn get_Color(&self, value: *mut windows::UI::Color) -> HRESULT;
    pub fn put_Color(&self, value: windows::UI::Color) -> HRESULT;
}

/// `Windows.Foundation.TimeSpan`: a duration in 100 ns units.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeSpan {
    pub Duration: i64,
}

/// `Windows.UI.Xaml.Media.IAcrylicBrush`, from the SDK's
/// `winrt/windows.ui.xaml.media.h` (members in its order). As above, the
/// first three slots are `IInspectable`'s. `AcrylicBackgroundSource` is an
/// `i32` enum (0 HostBackdrop, 1 Backdrop); `boolean` is one byte.
#[interface("79bbcf4e-cd66-4f1b-a8b6-cd6d2977c18d")]
pub unsafe trait IAcrylicBrush: IUnknown {
    pub fn GetIids(&self, count: *mut u32, iids: *mut *mut GUID) -> HRESULT;
    pub fn GetRuntimeClassName(&self, name: *mut *mut c_void) -> HRESULT;
    pub fn GetTrustLevel(&self, level: *mut i32) -> HRESULT;
    pub fn get_BackgroundSource(&self, value: *mut i32) -> HRESULT;
    pub fn put_BackgroundSource(&self, value: i32) -> HRESULT;
    pub fn get_TintColor(&self, value: *mut windows::UI::Color) -> HRESULT;
    pub fn put_TintColor(&self, value: windows::UI::Color) -> HRESULT;
    pub fn get_TintOpacity(&self, value: *mut f64) -> HRESULT;
    pub fn put_TintOpacity(&self, value: f64) -> HRESULT;
    pub fn get_TintTransitionDuration(&self, value: *mut TimeSpan) -> HRESULT;
    pub fn put_TintTransitionDuration(&self, value: TimeSpan) -> HRESULT;
    pub fn get_AlwaysUseFallback(&self, value: *mut u8) -> HRESULT;
    pub fn put_AlwaysUseFallback(&self, value: u8) -> HRESULT;
}

/// `[in] BSTR` argument: the string's pointer, or null for an empty one.
pub fn bstr_arg(value: &BSTR) -> *const u16 {
    if value.is_empty() {
        core::ptr::null()
    } else {
        value.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn empty_bstr_is_passed_as_null() {
        assert!(bstr_arg(&BSTR::new()).is_null());
        let text = BSTR::from("Fill");
        assert_eq!(bstr_arg(&text), text.as_ptr());
    }
}
