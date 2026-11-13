//! Drives the DLL end to end against a fake XAML Diagnostics service that
//! records every call: class factory, `SetSite`, tree callbacks, commands
//! through the control window, re-attach and detach. Windows only (it uses
//! real COM objects and a real message-only window); runs in CI.

use std::{
    thread,
    time::{Duration, Instant},
};

use sepiida_core::{
    Color,
    shell::protocol::{CONTROL_WINDOW_CLASS, Command, TAP_CLSID},
    shell::rules::{RECTANGLE_TYPE, Role, TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::{Com::IClassFactory, Ole::IObjectWithSite, Threading::GetCurrentProcessId},
        UI::WindowsAndMessaging::{
            FindWindowExW, GetWindowThreadProcessId, HWND_MESSAGE, SendMessageW,
        },
    },
    core::{BSTR, GUID, HSTRING, IUnknown, Interface, PCWSTR},
};

use crate::{DllGetClassObject, tap::Factory, xamlom::*};

mod fake;

use fake::*;

const TASKBAR_BACKGROUND_NAME: &str = TASKBAR_BACKGROUND.name;
const BRUSH: &str = "Windows.UI.Xaml.Media.SolidColorBrush";

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
    send(Command::SetColor(
        Role::Surface,
        Color::new(0x33, 0x66, 0x99),
    ));
    assert!(first.has(&format!("CreateInstance {BRUSH} null -> 0x1000")));
    assert!(first.has("GetPropertyValuesChain 0x1000"));
    assert!(first.has("CreateInstance Windows.UI.Color #FF336699 -> 0x1001"));
    assert!(first.has(&format!("SetProperty 0x1000 {COLOR_INDEX} 0x1001")));
    assert!(first.has(&format!("SetProperty 0x5 {FILL_INDEX} 0x1000")));

    // A second colour reuses the brush.
    send(Command::SetColor(Role::Surface, Color::new(1, 2, 3)));
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
    send(Command::SetColor(Role::Surface, Color::new(9, 9, 9)));
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
    send(Command::SetColor(Role::Surface, Color::new(4, 5, 6)));
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
