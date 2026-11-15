//! Keeps the glass on while Explorer works: its own calls that would undo
//! it are rewritten, as the Windhawk File Explorer Styler's
//! `DwmSetWindowAttribute_Hook` and `DwmExtendFrameIntoClientArea_Hook`
//! do (GPL-3.0). For a window with our glass, a system backdrop or host
//! backdrop brush becomes `DWMSBT_AUTO` (so the accent blur stays) and any
//! margins become the whole window. Every other call passes through
//! untouched.
//!
//! Installed (import-table patches in every module of the process) when
//! glass goes on, removed once no window of the process has it. The hooks
//! run on any thread of explorer.exe, so they keep no state but atomics.

use std::{
    ffi::c_void,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

use sepiida_core::pe::Import;
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND},
        Graphics::Dwm::{DWMSBT_AUTO, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_HOSTBACKDROPBRUSH},
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::Controls::MARGINS,
    },
    core::{HRESULT, PCSTR, s, w},
};

use super::glass::{has_glass, process_has_glass};
use crate::{
    guard,
    imports::{self, HookSet, Replacement},
    log::log,
};

/// The DWM hooks; `dwmapi` also matches API sets such as
/// `ext-ms-win-dwmapi-ext-*`.
const DWM_HOOKS: HookSet = HookSet {
    name: "dwmapi",
    dlls: &["dwmapi"],
    implementers: &["dwmapi"],
    only_in: None,
};
/// Calls on glass windows logged per process: enough to see what Explorer
/// does over many activations, few enough to keep the log small.
const LOGGED_CALLS: u32 = 300;

type SetWindowAttributeFn = unsafe extern "system" fn(HWND, u32, *const c_void, u32) -> HRESULT;
type ExtendFrameFn = unsafe extern "system" fn(HWND, *const MARGINS) -> HRESULT;

/// Calls rewritten so far (for tests and the log).
pub(super) static REWRITES: AtomicU32 = AtomicU32::new(0);
static LOGGED: AtomicU32 = AtomicU32::new(0);

/// Hooks Explorer's DWM calls in every module loaded now (again after
/// new modules loaded; slots already ours are skipped).
pub fn install() {
    if real_set_window_attribute().is_none() || real_extend_frame().is_none() {
        log!("backdrop hooks: dwmapi functions not found; not hooked");
        return;
    }
    let count = imports::patch(
        &DWM_HOOKS,
        &[
            Replacement {
                function: Import::Name("DwmSetWindowAttribute"),
                address: set_window_attribute_hook as *const () as usize,
            },
            Replacement {
                function: Import::Name("DwmExtendFrameIntoClientArea"),
                address: extend_frame_hook as *const () as usize,
            },
        ],
    );
    if count > 0 {
        log!("backdrop hooks: {count} import slot(s) hooked");
    } else if !imports::is_patched(DWM_HOOKS.name) {
        log!("backdrop hooks: no module imports the DWM functions; nothing hooked");
    }
}

/// Removes the hooks once no window of this process has the glass.
pub fn uninstall_if_unused() {
    if imports::is_patched(DWM_HOOKS.name) && !process_has_glass() {
        let count = imports::unpatch(DWM_HOOKS.name);
        log!("backdrop hooks: {count} import slot(s) put back");
    }
}

fn log_call(what: impl FnOnce() -> String) {
    if LOGGED.fetch_add(1, Ordering::Relaxed) < LOGGED_CALLS {
        log!("backdrop hook: {}", what());
    }
}

/// What a hook does with a call.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    PassThrough,
    Rewrite,
}

unsafe extern "system" fn set_window_attribute_hook(
    window: HWND,
    attribute: u32,
    value: *const c_void,
    size: u32,
) -> HRESULT {
    let Some(real) = real_set_window_attribute() else {
        return E_FAIL;
    };
    let verdict = guard("DwmSetWindowAttribute hook", Verdict::PassThrough, || {
        if !has_glass(window) {
            return Verdict::PassThrough;
        }
        let backdrop = attribute == DWMWA_SYSTEMBACKDROP_TYPE.0 as u32
            || attribute == DWMWA_USE_HOSTBACKDROPBRUSH.0 as u32;
        let verdict = if backdrop {
            REWRITES.fetch_add(1, Ordering::Relaxed);
            Verdict::Rewrite
        } else {
            Verdict::PassThrough
        };
        log_call(|| {
            let shown = if value.is_null() || size < 4 {
                "?".to_owned()
            } else {
                // SAFETY: the caller passes `size` readable bytes at `value`.
                unsafe { value.cast::<u32>().read_unaligned() }.to_string()
            };
            let done = if backdrop {
                "kept as glass"
            } else {
                "passed on"
            };
            format!("{window:?}: DwmSetWindowAttribute({attribute}, {shown}) {done}")
        });
        verdict
    });
    if verdict == Verdict::PassThrough {
        // SAFETY: the caller's arguments, passed on unchanged.
        return unsafe { real(window, attribute, value, size) };
    }
    let kind = DWMSBT_AUTO;
    // SAFETY: `kind` is the attribute's type and lives across the call.
    unsafe {
        real(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE.0 as u32,
            (&raw const kind).cast(),
            size_of_val(&kind) as u32,
        )
    }
}

unsafe extern "system" fn extend_frame_hook(window: HWND, margins: *const MARGINS) -> HRESULT {
    let Some(real) = real_extend_frame() else {
        return E_FAIL;
    };
    let verdict = guard(
        "DwmExtendFrameIntoClientArea hook",
        Verdict::PassThrough,
        || {
            if !has_glass(window) {
                return Verdict::PassThrough;
            }
            REWRITES.fetch_add(1, Ordering::Relaxed);
            log_call(|| {
                // SAFETY: the caller passes a MARGINS pointer or null.
                let shown = unsafe { margins.as_ref() }.map_or("null".to_owned(), |m| {
                    format!(
                        "{}, {}, {}, {}",
                        m.cxLeftWidth, m.cxRightWidth, m.cyTopHeight, m.cyBottomHeight
                    )
                });
                format!("{window:?}: DwmExtendFrameIntoClientArea({shown}) kept whole")
            });
            Verdict::Rewrite
        },
    );
    if verdict == Verdict::PassThrough {
        // SAFETY: the caller's arguments, passed on unchanged.
        return unsafe { real(window, margins) };
    }
    let whole = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: `whole` lives across the call.
    unsafe { real(window, &whole) }
}

fn real_set_window_attribute() -> Option<SetWindowAttributeFn> {
    static REAL: OnceLock<Option<SetWindowAttributeFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        // SAFETY: the export has had this signature since Windows Vista.
        dwmapi_export(s!("DwmSetWindowAttribute")).map(|f| unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, SetWindowAttributeFn>(f)
        })
    })
}

fn real_extend_frame() -> Option<ExtendFrameFn> {
    static REAL: OnceLock<Option<ExtendFrameFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        // SAFETY: the export has had this signature since Windows Vista.
        dwmapi_export(s!("DwmExtendFrameIntoClientArea")).map(|f| unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, ExtendFrameFn>(f)
        })
    })
}

/// A dwmapi export. dwmapi is loaded: this DLL imports from it.
fn dwmapi_export(name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY: looks up a loaded module and one of its exports by name.
    unsafe {
        let module = GetModuleHandleW(w!("dwmapi.dll")).ok()?;
        GetProcAddress(module, name)
    }
}

#[cfg(all(test, windows))]
mod tests;
