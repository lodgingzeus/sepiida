//! This DLL's own module: its handle, its path, and pinning it in memory.

use std::path::PathBuf;

use windows::{
    Win32::{
        Foundation::HMODULE,
        System::LibraryLoader::{
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
        },
    },
    core::PCWSTR,
};

use crate::log::log;

/// Any address inside this DLL identifies the module.
fn anchor() -> PCWSTR {
    PCWSTR(this_module as *const u16)
}

fn module_handle(flags: u32) -> Option<HMODULE> {
    let mut module = HMODULE::default();
    // SAFETY: `anchor` is an address inside this DLL; `module` is writable.
    match unsafe {
        GetModuleHandleExW(
            flags | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            anchor(),
            &mut module,
        )
    } {
        Ok(()) => Some(module),
        Err(error) => {
            log!("GetModuleHandleExW({flags:#x}) failed: {error}");
            None
        }
    }
}

/// This DLL's module handle, without changing its reference count.
pub fn this_module() -> Option<HMODULE> {
    module_handle(GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT)
}

/// Keeps this DLL loaded until the process exits, whatever else frees it.
/// Needed before starting a thread from a hook procedure: removing the hook
/// would otherwise unload the DLL under that thread.
pub fn pin() -> bool {
    module_handle(GET_MODULE_HANDLE_EX_FLAG_PIN).is_some()
}

/// Full path of this DLL, as handed to `InitializeXamlDiagnosticsEx`.
pub fn this_path() -> Option<PathBuf> {
    let module = this_module()?;
    let mut buffer = vec![0u16; 1024];
    // SAFETY: the buffer is writable for its whole length.
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    // A full buffer means the path was cut off.
    if len == 0 || len >= buffer.len() {
        log!("GetModuleFileNameW failed (length {len})");
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}
