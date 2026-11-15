//! Calls the hook functions directly, as a patched Explorer module would.

use windows::{
    Win32::{
        Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, RemovePropW,
            SetPropW, WINDOW_EX_STYLE, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
        },
    },
    core::{BOOL, HSTRING, PCWSTR},
};

use super::*;

extern "system" fn default_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: forwarding the arguments we received.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn window() -> HWND {
    // SAFETY: the test executable's own handle.
    let instance: HINSTANCE = unsafe { GetModuleHandleW(PCWSTR::null()) }.unwrap().into();
    let class = HSTRING::from("sepiida.test.hooked");
    let info = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(default_proc),
        hInstance: instance,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // SAFETY: fully initialised; a second registration fails harmlessly.
    unsafe { RegisterClassExW(&info) };
    // SAFETY: the class is registered above; the window stays hidden.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class,
            &class,
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .unwrap()
}

fn mark(window: HWND, on: bool) {
    let prop = windows::core::w!("sepiida.backdrop");
    // SAFETY: our own test window and property.
    unsafe {
        if on {
            SetPropW(
                window,
                prop,
                Some(windows::Win32::Foundation::HANDLE(
                    std::ptr::without_provenance_mut(1),
                )),
            )
            .unwrap();
        } else {
            let _ = RemovePropW(window, prop);
        }
    }
}

fn rewrites() -> u32 {
    REWRITES.load(Ordering::Relaxed)
}

#[test]
fn calls_on_glass_windows_are_rewritten_and_others_pass_through() {
    let _lock = imports::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let glass = window();
    let plain = window();
    mark(glass, true);
    let host_backdrop = BOOL(1);
    let value = (&raw const host_backdrop).cast();
    let whole = MARGINS {
        cxLeftWidth: 0,
        cxRightWidth: 0,
        cyTopHeight: 40,
        cyBottomHeight: 0,
    };

    let before = rewrites();
    // SAFETY: valid windows and arguments, as a caller would pass them.
    unsafe {
        let _ = set_window_attribute_hook(plain, DWMWA_USE_HOSTBACKDROPBRUSH.0 as u32, value, 4);
        let _ = extend_frame_hook(plain, &whole);
    }
    assert_eq!(rewrites(), before, "a plain window's calls pass through");

    // SAFETY: as above.
    unsafe {
        let _ = set_window_attribute_hook(glass, DWMWA_USE_HOSTBACKDROPBRUSH.0 as u32, value, 4);
        let _ = set_window_attribute_hook(glass, DWMWA_SYSTEMBACKDROP_TYPE.0 as u32, value, 4);
        let _ = extend_frame_hook(glass, &whole);
        let _ = extend_frame_hook(glass, std::ptr::null());
        // Other attributes of a glass window pass through.
        let _ = set_window_attribute_hook(glass, 2, value, 4);
        let _ = set_window_attribute_hook(glass, 2, std::ptr::null(), 0);
    }
    assert_eq!(rewrites(), before + 4);

    mark(glass, false);
    // SAFETY: as above.
    unsafe {
        let _ = set_window_attribute_hook(glass, DWMWA_SYSTEMBACKDROP_TYPE.0 as u32, value, 4);
    }
    assert_eq!(rewrites(), before + 4, "no glass, no rewrite");

    for window in [glass, plain] {
        // SAFETY: created on this thread by the test.
        unsafe { DestroyWindow(window) }.unwrap();
    }
}
