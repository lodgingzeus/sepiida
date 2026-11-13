//! Shell styling DLL. sepiida.exe asks a shell process's XAML framework to
//! load it through XAML Diagnostics (`InitializeXamlDiagnosticsEx`); XAML
//! then creates our `IObjectWithSite` object and hands it the diagnostics
//! interfaces, through which we watch the visual tree and recolour the
//! taskbar background.
//!
//! Rules for everything here:
//! - No panic may cross the FFI boundary: every export, COM method and
//!   window procedure runs inside [`guard`]. A panic that escaped would
//!   abort Explorer.
//! - `DllMain` does nothing; the loader lock is held while it runs.
//! - XAML never unloads this DLL, so "detach" means: put back the original
//!   brushes, stop watching and hold no references.

#[cfg(windows)]
mod control;
#[cfg(windows)]
mod inject;
#[cfg(windows)]
mod log;
#[cfg(windows)]
mod module;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod styler;
#[cfg(windows)]
mod tap;
#[cfg(windows)]
mod watcher;
#[cfg(windows)]
mod xamlom;

#[cfg(all(test, windows))]
mod tests;

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Runs `body`, turning a panic into `fallback` (and a log line on
/// Windows) instead of letting it unwind into the caller.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "the DLL only does anything on Windows")
)]
pub(crate) fn guard<T>(context: &str, fallback: T, body: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("(no message)");
            #[cfg(windows)]
            log::log!("panic in {context}: {message}");
            #[cfg(not(windows))]
            let _ = (context, message);
            fallback
        }
    }
}

#[cfg(windows)]
pub use exports::*;

#[cfg(windows)]
mod exports {
    use core::ffi::c_void;

    use windows::{
        Win32::{
            Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_POINTER, E_UNEXPECTED, S_FALSE},
            System::Com::IClassFactory,
        },
        core::{GUID, HRESULT, Interface},
    };

    use crate::{guard, log::log, tap::Factory};

    #[unsafe(no_mangle)]
    pub extern "system" fn DllMain(
        _module: *mut c_void,
        _reason: u32,
        _reserved: *mut c_void,
    ) -> i32 {
        1
    }

    /// Called by XAML Diagnostics with the class ID sepiida.exe passed to
    /// `InitializeXamlDiagnosticsEx`.
    ///
    /// # Safety
    /// `clsid` and `iid` must be null or point to GUIDs; `out` must be null
    /// or writable.
    #[unsafe(no_mangle)]
    pub unsafe extern "system" fn DllGetClassObject(
        clsid: *const GUID,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> HRESULT {
        guard("DllGetClassObject", E_UNEXPECTED, || {
            if clsid.is_null() || iid.is_null() || out.is_null() {
                return E_POINTER;
            }
            // SAFETY: checked non-null above; the caller passes valid pointers.
            let hr = unsafe {
                *out = core::ptr::null_mut();
                if *clsid != GUID::from_u128(sepiida_core::shell::protocol::TAP_CLSID) {
                    CLASS_E_CLASSNOTAVAILABLE
                } else {
                    let factory: IClassFactory = Factory.into();
                    factory.query(iid, out)
                }
            };
            // SAFETY: checked non-null above.
            let (clsid, iid) = unsafe { (*clsid, *iid) };
            log!(
                "DllGetClassObject(clsid {clsid:?}, iid {iid:?}) = {:#010x}",
                hr.0
            );
            hr
        })
    }

    /// XAML keeps this DLL loaded for good, and we never want it unloaded
    /// while Explorer could still call into it.
    #[unsafe(no_mangle)]
    pub extern "system" fn DllCanUnloadNow() -> HRESULT {
        S_FALSE
    }
}

#[cfg(test)]
mod guard_tests {
    use super::guard;

    #[test]
    fn guard_returns_the_value() {
        assert_eq!(guard("test", 0, || 7), 7);
    }

    #[test]
    fn guard_turns_a_panic_into_the_fallback() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = guard("test", -1, || -> i32 { panic!("boom") });
        std::panic::set_hook(previous);
        assert_eq!(result, -1);
    }
}
