//! Drives the DLL end to end against a fake XAML Diagnostics service that
//! records every call: class factory, `SetSite`, tree callbacks, commands
//! through the control window, re-attach and detach. Windows only (it uses
//! real COM objects and a real message-only window); runs in CI.

use core::ffi::c_void;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color,
    shell::protocol::{CONTROL_WINDOW_CLASS, Command, TAP_CLSID},
    shell::tree::{RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME, TASKBAR_FRAME_NAME},
};
use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, E_NOTIMPL, ERROR_NOT_FOUND, HWND, LPARAM, RECT, S_OK, WPARAM},
        System::{
            Com::{CoTaskMemAlloc, IClassFactory},
            Ole::IObjectWithSite,
            Threading::GetCurrentProcessId,
        },
        UI::WindowsAndMessaging::{
            FindWindowExW, GetWindowThreadProcessId, HWND_MESSAGE, SendMessageW,
        },
    },
    core::{BOOL, BSTR, GUID, HRESULT, HSTRING, IUnknown, Interface, PCWSTR, implement},
};

use crate::{DllGetClassObject, tap::Factory, xamlom::*};

const FILL_INDEX: u32 = 7;
const COLOR_INDEX: u32 = 9;
const ORIGINAL_FILL: InstanceHandle = 0x100;
/// The handle the original brush gets once its first handle went stale.
const LIVE_FILL: InstanceHandle = 0x200;
const RECT_HANDLE: InstanceHandle = 5;

struct SendCallback(IVisualTreeServiceCallback);
// SAFETY: test-only; the callback is only called from the test thread.
unsafe impl Send for SendCallback {}

#[derive(Default)]
struct Shared {
    calls: Mutex<Vec<String>>,
    callback: Mutex<Option<SendCallback>>,
    values: Mutex<HashMap<(InstanceHandle, u32), InstanceHandle>>,
    next_handle: AtomicU64,
}

impl Shared {
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn has(&self, call: &str) -> bool {
        self.calls().iter().any(|c| c == call)
    }

    fn count(&self, call: &str) -> usize {
        self.calls().iter().filter(|c| *c == call).count()
    }
}

#[implement(IXamlDiagnostics, IVisualTreeService3)]
struct FakeXaml(Arc<Shared>);

fn text(p: *const u16) -> String {
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
        if handle != ORIGINAL_FILL {
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

const BRUSH: &str = "Windows.UI.Xaml.Media.SolidColorBrush";

fn chain_value(index: u32, name: &str, type_name: &str) -> PropertyChainValue {
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

fn empty_source_info() -> SourceInfo {
    SourceInfo {
        FileName: BSTR::new(),
        LineNumber: 0,
        ColumnNumber: 0,
        CharPosition: 0,
        Hash: BSTR::new(),
    }
}

fn new_fake() -> (Arc<Shared>, IUnknown) {
    let shared = Arc::new(Shared::default());
    shared
        .values
        .lock()
        .unwrap()
        .insert((RECT_HANDLE, FILL_INDEX), ORIGINAL_FILL);
    let site: IUnknown = FakeXaml(shared.clone()).into();
    (shared, site)
}

/// Creates the TAP object the way XAML does: through the exported
/// `DllGetClassObject` and the class factory.
fn new_tap() -> IObjectWithSite {
    let mut raw = core::ptr::null_mut();
    // SAFETY: valid GUID pointers and a writable out pointer.
    let hr =
        unsafe { DllGetClassObject(&GUID::from_u128(TAP_CLSID), &IClassFactory::IID, &mut raw) };
    hr.unwrap();
    // SAFETY: `raw` is an owned IClassFactory on success.
    let factory = unsafe { IClassFactory::from_raw(raw) };
    // SAFETY: plain COM call.
    unsafe { factory.CreateInstance::<Option<&IUnknown>, IObjectWithSite>(None) }.unwrap()
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_callback(shared: &Shared) -> IVisualTreeServiceCallback2 {
    wait_until("Advise", || shared.callback.lock().unwrap().is_some());
    let callback = shared.callback.lock().unwrap().take().unwrap().0;
    // Also checks QueryInterface across the callback interface chain.
    callback.cast().unwrap()
}

fn add(callback: &IVisualTreeServiceCallback2, handle: u64, parent: u64, ty: &str, name: &str) {
    let relation = ParentChildRelation {
        Parent: parent,
        Child: handle,
        ChildIndex: 0,
    };
    let element = VisualElement {
        Handle: handle,
        SrcInfo: empty_source_info(),
        Type: BSTR::from(ty),
        Name: BSTR::from(name),
        NumChildren: 0,
    };
    // SAFETY: plain COM call; the element's strings move to the callee.
    let hr = unsafe { callback.OnVisualTreeChange(relation, element, VisualMutationType::ADD) };
    hr.unwrap();
}

/// TaskbarFrame(1) > Grid(2) > TaskbarBackground(3) > Grid(4) >
/// Rectangle#BackgroundFill(5), with the intermediate grid arriving last.
fn add_taskbar(callback: &IVisualTreeServiceCallback2) {
    let grid = "Windows.UI.Xaml.Controls.Grid";
    add(callback, 1, 0, "Taskbar.TaskbarFrame", TASKBAR_FRAME_NAME);
    add(
        callback,
        3,
        2,
        "Taskbar.TaskbarBackground",
        "BackgroundControl",
    );
    add(callback, 4, 3, grid, "");
    add(
        callback,
        RECT_HANDLE,
        4,
        RECTANGLE_TYPE,
        TASKBAR_BACKGROUND_NAME,
    );
    add(callback, 2, 1, grid, "RootGrid");
}

fn control_window() -> Option<HWND> {
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let mut after = None;
    // SAFETY: plain Win32 calls.
    unsafe {
        while let Ok(window) = FindWindowExW(Some(HWND_MESSAGE), after, &class, PCWSTR::null()) {
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == GetCurrentProcessId() {
                return Some(window);
            }
            after = Some(window);
        }
    }
    None
}

fn send(command: Command) {
    let window = control_window().expect("control window");
    let (message, wparam) = command.encode();
    // SAFETY: our own window on this thread; handled synchronously.
    unsafe { SendMessageW(window, message, Some(WPARAM(wparam)), Some(LPARAM(0))) };
}

#[test]
fn attach_style_restore_reattach_detach() {
    // Attach.
    let (first, site) = new_fake();
    let tap = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap.SetSite(&site) }.unwrap();
    let callback = wait_for_callback(&first);
    add_taskbar(&callback);
    assert!(first.has("GetPropertyValuesChain 0x5"));
    assert!(control_window().is_some());

    // Colour: shared brush, its colour set through the diagnostics API.
    send(Command::SetColor(Color::new(0x33, 0x66, 0x99)));
    assert!(first.has(&format!("CreateInstance {BRUSH} null -> 0x1000")));
    assert!(first.has("GetPropertyValuesChain 0x1000"));
    assert!(first.has("CreateInstance Windows.UI.Color #FF336699 -> 0x1001"));
    assert!(first.has(&format!("SetProperty 0x1000 {COLOR_INDEX} 0x1001")));
    assert!(first.has(&format!("SetProperty 0x5 {FILL_INDEX} 0x1000")));

    // A second colour reuses the brush.
    send(Command::SetColor(Color::new(1, 2, 3)));
    assert_eq!(
        first.count(&format!("CreateInstance {BRUSH} null -> 0x1000")),
        1
    );
    assert!(first.has("CreateInstance Windows.UI.Color #FF010203 -> 0x1002"));

    // Restore puts the original brush back through a fresh handle: the
    // recorded one went stale once the brush was replaced.
    let restore = format!("SetProperty 0x5 {FILL_INDEX} {LIVE_FILL:#x}");
    send(Command::Restore);
    assert_eq!(first.count(&restore), 1);
    // Nothing styled: a second restore does nothing.
    send(Command::Restore);
    assert_eq!(first.count(&restore), 1);

    // Attach again without detaching: the first attachment is unadvised and
    // its styled taskbar restored before the new one takes over.
    send(Command::SetColor(Color::new(9, 9, 9)));
    let (second, site2) = new_fake();
    let tap2 = new_tap();
    // SAFETY: plain COM call.
    unsafe { tap2.SetSite(&site2) }.unwrap();
    wait_until("Unadvise of the first attachment", || first.has("Unadvise"));
    let callback2 = wait_for_callback(&second);
    add_taskbar(&callback2);
    assert_eq!(first.count(&restore), 2);
    assert!(second.has("GetPropertyValuesChain 0x5"));
    // Late callbacks for the old attachment are ignored.
    let before = first.calls().len();
    add(&callback, 50, 0, RECTANGLE_TYPE, TASKBAR_BACKGROUND_NAME);
    assert_eq!(first.calls().len(), before);

    // Detach: restore, unadvise, window gone.
    send(Command::SetColor(Color::new(4, 5, 6)));
    send(Command::Detach);
    assert!(second.has(&format!("SetProperty 0x5 {FILL_INDEX} {LIVE_FILL:#x}")));
    wait_until("Unadvise of the second attachment", || {
        second.has("Unadvise")
    });
    assert!(control_window().is_none());
}

#[test]
fn a_site_without_the_diagnostics_interfaces_is_refused() {
    let tap = new_tap();
    let not_xaml: IUnknown = Factory.into();
    // SAFETY: plain COM call.
    assert!(unsafe { tap.SetSite(&not_xaml) }.is_err());
}

#[test]
fn other_class_ids_are_refused() {
    let mut raw = core::ptr::null_mut();
    // SAFETY: valid GUID pointers and a writable out pointer.
    let hr = unsafe { DllGetClassObject(&GUID::zeroed(), &IClassFactory::IID, &mut raw) };
    assert!(hr.is_err());
    assert!(raw.is_null());
    // SAFETY: null pointers must be rejected, not dereferenced.
    let hr = unsafe { DllGetClassObject(core::ptr::null(), &IClassFactory::IID, &mut raw) };
    assert!(hr.is_err());
}
