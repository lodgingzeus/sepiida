//! Which DLLs another process has loaded (Toolhelp module snapshot).

use std::path::PathBuf;

use windows::Win32::{
    Foundation::CloseHandle,
    System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    },
};

/// The full path of the module named `file_name` (case-insensitive) in
/// process `pid`, or `None` if that process has not loaded it.
pub fn loaded_module(pid: u32, file_name: &str) -> windows::core::Result<Option<PathBuf>> {
    // SAFETY: the snapshot handle is closed below; the entry struct is
    // initialised with its size as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?;
        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut more = Module32FirstW(snapshot, &mut entry).is_ok();
        while more {
            if wide_to_string(&entry.szModule).eq_ignore_ascii_case(file_name) {
                found = Some(PathBuf::from(wide_to_string(&entry.szExePath)));
                break;
            }
            more = Module32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Ok(found)
    }
}

/// A NUL-terminated UTF-16 buffer as a string.
fn wide_to_string(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}
