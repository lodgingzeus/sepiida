//! Listens for the window messages Windows broadcasts when the desktop
//! wallpaper or the monitor layout changes.
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
            SPI_SETDESKWALLPAPER, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DISPLAYCHANGE,
            WM_SETTINGCHANGE, WNDCLASSEXW,
        },
    },
    core::w,
};

thread_local! {
    /// Where the window procedure sends events. Set once, on the window's
    /// own thread, before the window exists.
    static SENDER: OnceCell<Sender<Event>> = const { OnceCell::new() };
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

/// Starts the listener. Returns once the window exists, so no change after
/// this call is missed. The thread runs until the process exits.
pub fn spawn(sender: Sender<Event>) -> Result<(), DesktopEventsError> {
    let (ready_tx, ready_rx) = mpsc::channel();
    thread::Builder::new()
        .name("desktop-events".to_owned())
        .spawn(move || {
            SENDER.with(|cell| {
                // The cell is new on this thread, so it cannot be set yet.
                let _ = cell.set(sender);
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
    if is_wallpaper_change(message, wparam) {
        SENDER.with(|cell| {
            if let Some(sender) = cell.get() {
                // The receiver only goes away when the agent is quitting.
                let _ = sender.send(Event::Changed);
            }
        });
    }
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
