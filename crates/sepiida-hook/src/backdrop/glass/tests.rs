use windows::{
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, WINDOW_EX_STYLE,
        WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW,
    },
    core::HSTRING,
};

use super::*;

extern "system" fn default_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: forwarding the arguments we received.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn instance() -> windows::Win32::Foundation::HINSTANCE {
    // SAFETY: the test executable's own handle.
    unsafe { GetModuleHandleW(PCWSTR::null()) }.unwrap().into()
}

fn register(class: &HSTRING) {
    let info = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(default_proc),
        hInstance: instance(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // SAFETY: fully initialised; a second registration fails harmlessly.
    unsafe { RegisterClassExW(&info) };
}

fn create(class: &str, parent: Option<HWND>) -> HWND {
    let class = HSTRING::from(class);
    register(&class);
    let style = if parent.is_some() {
        WS_CHILD
    } else {
        WS_OVERLAPPEDWINDOW
    };
    // SAFETY: the class is registered above; the window is hidden.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class,
            &class,
            style,
            0,
            0,
            200,
            200,
            parent,
            None,
            Some(instance()),
            None,
        )
    }
    .unwrap()
}

fn destroy(window: HWND) {
    // SAFETY: created on this thread by the test.
    unsafe { DestroyWindow(window) }.unwrap();
}

#[test]
fn folder_windows_get_the_glass_and_lose_it_on_restore() {
    let _lock = crate::imports::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let folder = create("CabinetWClass", None);
    let legacy = create("CabinetWClass", None);
    create("WorkerW", Some(legacy));
    let other = create("sepiida.test.other", None);
    assert_eq!(glass_windows(), [folder]);

    let mut glass = WindowBackdrop {
        state: Backdrop::default(),
        enabled: true,
    };
    glass.set_look(true);
    assert!(has_glass(folder));
    assert!(!has_glass(legacy));
    glass.set_tint(Color::new(0x33, 0x66, 0x99));
    glass.refresh();
    glass.restore();
    assert!(!has_glass(folder));
    flush();
    PENDING_UPDATES.with(|pending| assert!(pending.borrow().is_empty()));
    // The last glass is gone, so are the hooks.
    assert!(!crate::imports::is_patched("dwmapi"));

    for window in [folder, legacy, other] {
        destroy(window);
    }
}

#[test]
fn the_taskbars_thread_gets_no_glass() {
    let _lock = crate::imports::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let folder = create("CabinetWClass", None);
    let mut glass = WindowBackdrop {
        state: Backdrop::default(),
        enabled: false,
    };
    glass.set_look(true);
    assert!(!has_glass(folder));
    destroy(folder);
}
