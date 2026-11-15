//! A message-only window on the XAML UI thread. sepiida.exe posts commands
//! to it; its window procedure runs on this thread, which is where XAML
//! elements may be changed.
//!
//! Any process on the desktop can post to the window, so every message is
//! decoded strictly (see `shell::protocol::Command::decode`) and anything
//! else goes to `DefWindowProcW`.
//!
//! The title names the framework the thread's XAML is (system XAML or
//! WinUI 3), so sepiida.exe can address the taskbar's copy and File
//! Explorer's apart in one explorer.exe.

use std::cell::Cell;

use sepiida_core::shell::{
    framework::Framework,
    protocol::{CONTROL_WINDOW_CLASS, Command, control_window_title},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, KillTimer,
            RegisterClassExW, SetTimer, WINDOW_EX_STYLE, WINDOW_STYLE, WM_TIMER, WNDCLASSEXW,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::{guard, log::log, module, watcher};

/// Timer that re-measures the look's layout once the tree has settled.
const RELAYOUT_TIMER: usize = 1;
/// Quiet time after the last tree change before re-measuring: long enough
/// for XAML's layout pass, short enough not to be seen.
const RELAYOUT_DELAY_MS: u32 = 300;

thread_local! {
    static WINDOW: Cell<Option<HWND>> = const { Cell::new(None) };
    /// The framework this thread's window serves.
    static FRAMEWORK: Cell<Framework> = const { Cell::new(Framework::System) };
}

/// The framework this thread's control window serves (system XAML if it
/// has none).
pub fn framework() -> Framework {
    FRAMEWORK.with(Cell::get)
}

/// Creates this thread's control window for `framework` unless it exists.
pub fn ensure_window(framework: Framework) {
    if WINDOW.with(Cell::get).is_some() {
        return;
    }
    FRAMEWORK.with(|f| f.set(framework));
    let Some(instance) = module::this_module().map(HINSTANCE::from) else {
        return;
    };
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let title = HSTRING::from(control_window_title(framework));
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
            &title,
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
            log!("control window created ({framework:?} XAML)");
            WINDOW.with(|w| w.set(Some(window)));
        }
        Err(error) => log!("could not create the control window: {error}"),
    }
}

/// Re-measures the look's layout `RELAYOUT_DELAY_MS` from now; a later call
/// restarts the wait.
pub fn schedule_relayout() {
    if let Some(window) = WINDOW.with(Cell::get) {
        // SAFETY: our own window, on its thread.
        if unsafe { SetTimer(Some(window), RELAYOUT_TIMER, RELAYOUT_DELAY_MS, None) } == 0 {
            log!("SetTimer failed");
        }
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
        if message == WM_TIMER && wparam.0 == RELAYOUT_TIMER {
            // SAFETY: our own window and timer.
            let _ = unsafe { KillTimer(Some(window), RELAYOUT_TIMER) };
            watcher::relayout();
            return LRESULT(0);
        }
        match Command::decode(message, wparam.0) {
            Some(command) => {
                watcher::run_command(command);
                LRESULT(0)
            }
            None => default(),
        }
    })
}
