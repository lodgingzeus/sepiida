//! Listens for the window messages Windows broadcasts when the desktop
//! wallpaper or the monitor layout changes, and when Explorer restarts
//! (`TaskbarCreated`, after which tray icons must be added again).
//!
//! Broadcasts only reach top-level windows (message-only windows do not get
//! them), so this creates an invisible top-level window on its own thread.

use std::{
    cell::OnceCell,
    sync::mpsc::{self, Sender},
    thread,
};

use sepiida_core::schedule::Event;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassExW,
            RegisterWindowMessageW, SPI_SETDESKWALLPAPER, WINDOW_EX_STYLE, WINDOW_STYLE,
            WM_DISPLAYCHANGE, WM_SETTINGCHANGE, WNDCLASSEXW,
        },
    },
    core::w,
};

/// What the window procedure needs. Set once, on the window's own thread,
/// before the window exists.
struct Listener {
    sender: Sender<Event>,
    on_taskbar_created: Box<dyn Fn()>,
    /// The id Windows assigned to the `TaskbarCreated` message; 0 if it
    /// could not be registered, which matches no message.
    taskbar_created: u32,
}

thread_local! {
    static LISTENER: OnceCell<Listener> = const { OnceCell::new() };
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopEventsError {
    #[error("could not start the desktop event thread")]
    Thread(#[source] std::io::Error),
    #[error("could not create the desktop event window")]
    Window(#[source] windows::core::Error),
    #[error("the desktop event thread stopped during startup")]
    Startup,
}

/// Starts the listener. Wallpaper changes go to `sender`;
/// `on_taskbar_created` runs on the listener thread after an Explorer
/// restart. Returns once the window exists, so no change after this call
/// is missed. The thread runs until the process exits.
pub fn spawn(
    sender: Sender<Event>,
    on_taskbar_created: impl Fn() + Send + 'static,
) -> Result<(), DesktopEventsError> {
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("desktop-events".to_owned())
        .spawn(move || {
            // SAFETY: a static, null-terminated literal. Every program that
            // registers this name gets the same id.
            let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
            LISTENER.with(|cell| {
                // The cell is new on this thread, so it cannot be set yet.
                let _ = cell.set(Listener {
                    sender,
                    on_taskbar_created: Box::new(on_taskbar_created),
                    taskbar_created,
                });
            });
            let created = create_window();
            let ok = created.is_ok();
            let _ = ready_tx.send(created);
            if ok {
                message_loop();
            }
        })
        .map_err(DesktopEventsError::Thread)?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(DesktopEventsError::Window(error)),
        Err(_) => Err(DesktopEventsError::Startup),
    }
}

fn create_window() -> windows::core::Result<()> {
    let class_name = w!("sepiida.desktop-events");
    // SAFETY: plain Win32 calls with a static class name and a window
    // procedure that lives for the whole program. The window is never
    // shown and is destroyed when the process exits.
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("sepiida"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )?;
    }
    Ok(())
}

fn message_loop() {
    let mut message = MSG::default();
    // SAFETY: `message` is a valid MSG for every call. GetMessageW returns
    // 0 on WM_QUIT and -1 on error; both end the loop.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            DispatchMessageW(&message);
        }
    }
}

/// Which broadcasts mean the wallpaper to theme from may have changed.
fn is_wallpaper_change(message: u32, wparam: WPARAM) -> bool {
    match message {
        // Our own accent broadcast ("ImmersiveColorSet") has wParam 0, so
        // it never matches and cannot cause a loop.
        WM_SETTINGCHANGE => wparam.0 == SPI_SETDESKWALLPAPER.0 as usize,
        // The primary monitor, and with it the wallpaper sepiida reads, may
        // have changed.
        WM_DISPLAYCHANGE => true,
        _ => false,
    }
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    LISTENER.with(|cell| {
        let Some(listener) = cell.get() else { return };
        if is_wallpaper_change(message, wparam) {
            // The receiver only goes away when the agent is quitting.
            let _ = listener.sender.send(Event::Changed);
        } else if listener.taskbar_created != 0 && message == listener.taskbar_created {
            (listener.on_taskbar_created)();
        }
    });
    // SAFETY: forwards the message unchanged, as Windows expects.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{SPI_SETDESKPATTERN, WM_SYSCOLORCHANGE};

    use super::*;

    #[test]
    fn only_wallpaper_and_display_changes_count() {
        let wallpaper = WPARAM(SPI_SETDESKWALLPAPER.0 as usize);
        assert!(is_wallpaper_change(WM_SETTINGCHANGE, wallpaper));
        assert!(is_wallpaper_change(WM_DISPLAYCHANGE, WPARAM(0)));
        // Our own "ImmersiveColorSet" broadcast must not re-trigger us.
        assert!(!is_wallpaper_change(WM_SETTINGCHANGE, WPARAM(0)));
        assert!(!is_wallpaper_change(
            WM_SETTINGCHANGE,
            WPARAM(SPI_SETDESKPATTERN.0 as usize)
        ));
        assert!(!is_wallpaper_change(WM_SYSCOLORCHANGE, wallpaper));
    }
}
