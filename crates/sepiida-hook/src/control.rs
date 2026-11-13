//! A message-only window on the XAML UI thread. sepiida.exe posts commands
//! to it; its window procedure runs on this thread, which is where XAML
//! elements may be changed.
//!
//! Any process on the desktop can post to the window, so every message is
//! decoded strictly (see `shell::protocol::Command::decode`) and anything
//! else goes to `DefWindowProcW`.

use std::cell::Cell;

use sepiida_core::shell::protocol::{CONTROL_WINDOW_CLASS, Command};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::{
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            GetModuleHandleExW,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, RegisterClassExW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::{guard, log::log, watcher};

thread_local! {
    static WINDOW: Cell<Option<HWND>> = const { Cell::new(None) };
}

/// This DLL's module handle, which owns the window class.
fn module() -> Option<HINSTANCE> {
    let mut module = Default::default();
    // SAFETY: the address is a function in this DLL; the refcount is left
    // unchanged, and this DLL is never unloaded.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(window_proc as *const u16),
            &mut module,
        )
    };
    match found {
        Ok(()) => Some(module.into()),
        Err(error) => {
            log!("GetModuleHandleExW failed: {error}");
            None
        }
    }
}

/// Creates this thread's control window unless it exists.
pub fn ensure_window() {
    if WINDOW.with(Cell::get).is_some() {
        return;
    }
    let Some(instance) = module() else {
        return;
    };
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let class_info = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    // SAFETY: `class_info` is fully initialised; registering twice fails
    // harmlessly with ERROR_CLASS_ALREADY_EXISTS (a second UI thread).
    unsafe { RegisterClassExW(&class_info) };
    // SAFETY: the class is registered above (or already was).
    let created = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            &class,
            &class,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
    };
    match created {
        Ok(window) => {
            log!("control window created");
            WINDOW.with(|w| w.set(Some(window)));
        }
        Err(error) => log!("could not create the control window: {error}"),
    }
}

pub fn destroy_window() {
    if let Some(window) = WINDOW.with(Cell::take) {
        // SAFETY: the window was created on this thread.
        if let Err(error) = unsafe { DestroyWindow(window) } {
            log!("DestroyWindow failed: {error}");
        }
    }
}

extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY (fallback and default): forwarding the arguments we received.
    let default = || unsafe { DefWindowProcW(window, message, wparam, lparam) };
    guard("window procedure", LRESULT(0), || {
        match Command::decode(message, wparam.0) {
            Some(command) => {
                watcher::run_command(command);
                LRESULT(0)
            }
            None => default(),
        }
    })
}
