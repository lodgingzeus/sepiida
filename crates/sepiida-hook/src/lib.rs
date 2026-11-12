//! Shell styling DLL. It will be loaded into explorer.exe and the other shell
//! hosts through the XAML Diagnostics API; for now it is an empty shell so the
//! build and packaging pipeline has something to carry.
//!
//! Rule for everything added here: no panic may cross the FFI boundary. Wrap
//! every exported function and COM callback in `std::panic::catch_unwind`.

#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(
    _module: *mut core::ffi::c_void,
    _reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    1
}
