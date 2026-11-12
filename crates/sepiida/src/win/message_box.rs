//! Error reporting for the windowless program, which has no console.

use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::{HSTRING, w},
};

pub fn error(text: &str) {
    // SAFETY: both strings outlive the call; no owner window is needed.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            w!("sepiida"),
            MB_OK | MB_ICONERROR,
        );
    }
}
