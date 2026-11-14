//! Commands to the DLL: finding its control windows in a process and
//! posting to them.

use std::{
    thread,
    time::{Duration, Instant},
};

use sepiida_core::shell::protocol::{CONTROL_WINDOW_CLASS, Command, control_window_title};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            FindWindowExW, GetWindowThreadProcessId, HWND_MESSAGE, PostMessageW,
        },
    },
    core::{HSTRING, PCWSTR},
};

use super::ShellHookError;

/// How long `attach` waits for older copies to detach before going on.
const DETACH_TIMEOUT: Duration = Duration::from_secs(2);
/// Interval for the short waits on control windows coming and going.
const DETACH_POLL: Duration = Duration::from_millis(50);

/// Posts a command to every control window of this protocol version in
/// process `pid`. Returns how many received it.
pub fn send(pid: u32, command: Command) -> Result<usize, ShellHookError> {
    let title = control_window_title();
    let sent = post_to_windows(pid, Some(&title), command)?;
    if sent == 0 {
        return Err(ShellHookError::NotAttached);
    }
    Ok(sent)
}

/// Whether a copy of the DLL speaking this protocol version is attached in
/// process `pid`.
pub fn is_attached(pid: u32) -> bool {
    !control_windows(pid, Some(&control_window_title())).is_empty()
}

/// Waits up to `timeout` for the DLL's control window to appear in process
/// `pid`. The DLL reports ready once it watches the tree, but creates the
/// window only on the first tree callback, a moment later.
pub fn wait_until_attached(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if is_attached(pid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(DETACH_POLL);
    }
}

/// Asks every copy of the DLL in process `pid`, whatever its version, to
/// restore and detach. Returns how many control windows were asked.
pub fn detach_all(pid: u32) -> Result<usize, ShellHookError> {
    post_to_windows(pid, None, Command::Detach)
}

/// Waits a little for detached copies to destroy their control windows,
/// so their restore runs before a new copy starts styling.
pub(super) fn wait_for_detach(pid: u32) {
    let deadline = Instant::now() + DETACH_TIMEOUT;
    while !control_windows(pid, None).is_empty() && Instant::now() < deadline {
        thread::sleep(DETACH_POLL);
    }
}

pub(super) fn post_to_windows(
    pid: u32,
    title: Option<&str>,
    command: Command,
) -> Result<usize, ShellHookError> {
    let (message, wparam) = command.encode();
    let windows = control_windows(pid, title);
    for &window in &windows {
        // SAFETY: plain Win32 call; the handle is only used to post.
        unsafe { PostMessageW(Some(window), message, WPARAM(wparam), LPARAM(0)) }
            .map_err(ShellHookError::Post)?;
    }
    Ok(windows.len())
}

/// The DLL's message-only control windows owned by process `pid`; with a
/// title, only those of that protocol version.
pub(super) fn control_windows(pid: u32, title: Option<&str>) -> Vec<HWND> {
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let title = title.map(HSTRING::from);
    let title = title
        .as_ref()
        .map_or(PCWSTR::null(), |t| PCWSTR(t.as_ptr()));
    let mut found = Vec::new();
    let mut after: Option<HWND> = None;
    // SAFETY: plain Win32 calls; the strings outlive the loop.
    unsafe {
        while let Ok(window) = FindWindowExW(Some(HWND_MESSAGE), after, &class, title) {
            after = Some(window);
            let mut owner = 0;
            GetWindowThreadProcessId(window, Some(&mut owner));
            if owner == pid {
                found.push(window);
            }
        }
    }
    found
}
