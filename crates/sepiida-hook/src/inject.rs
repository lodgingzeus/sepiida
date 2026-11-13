//! Getting XAML to load this DLL and create our TAP object.
//!
//! `InitializeXamlDiagnosticsEx` (exported by `Windows.UI.Xaml.dll`) posts to
//! a XAML thread's diagnostics port, `VisualDiagConnection<N>`, in the
//! target process; XAML there loads this DLL by path and calls
//! `DllGetClassObject`. N is not known in advance, so the names are tried in
//! order until one is not "not found". XAML accepts one initialisation per
//! calling thread, so each attempt runs on a fresh thread.
//!
//! Two ways to reach it, both exported for sepiida.exe:
//! - Cross-process: sepiida.exe loads this DLL into itself and calls
//!   [`SepiidaInitializeXamlDiagnostics`] with Explorer's pid.
//! - In-process (TranslucentTB's current method): sepiida.exe installs
//!   [`SepiidaCallWndProc`] as a `WH_CALLWNDPROC` hook on the taskbar's
//!   thread only, which maps this DLL into Explorer, and sends the taskbar
//!   window `ATTACH_MESSAGE`. The hook procedure, now running inside
//!   Explorer, pins the DLL and calls `InitializeXamlDiagnosticsEx` for its
//!   own process from a new thread.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use sepiida_core::shell::protocol::{ATTACH_MESSAGE, MAX_CONNECTION, TAP_CLSID, connection_names};
use windows::{
    Win32::{
        Foundation::{E_POINTER, E_UNEXPECTED, ERROR_NOT_FOUND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            Threading::GetCurrentProcessId,
        },
        UI::WindowsAndMessaging::{CWPSTRUCT, CallNextHookEx, HC_ACTION, RegisterWindowMessageW},
    },
    core::{GUID, HRESULT, HSTRING, PCWSTR, s, w},
};

use crate::{guard, log::log, module};

type InitializeXamlDiagnosticsEx =
    unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> HRESULT;

/// Set while an in-process attach is running in this process.
static ATTACHING: AtomicBool = AtomicBool::new(false);

/// Asks XAML in process `pid` to load this DLL, trying connections from
/// `from` on. Returns the connection number that answered and its result.
pub fn initialize(pid: u32, from: u32) -> Result<(u32, HRESULT), HRESULT> {
    let Some(path) = module::this_path() else {
        return Err(E_UNEXPECTED);
    };
    let initialize = load_initialize()?;
    let not_found = ERROR_NOT_FOUND.to_hresult();
    let mut tried = 0;
    for (number, name) in connection_names(from) {
        tried += 1;
        let hr = call_on_fresh_thread(initialize, &name, pid, &path);
        if hr == not_found {
            continue;
        }
        log!(
            "InitializeXamlDiagnosticsEx(pid {pid}, {name}) = {:#010x} after {tried} attempt(s)",
            hr.0
        );
        return Ok((number, hr));
    }
    log!(
        "InitializeXamlDiagnosticsEx(pid {pid}): no connection from {from} answered ({tried} tried)"
    );
    Err(not_found)
}

fn load_initialize() -> Result<InitializeXamlDiagnosticsEx, HRESULT> {
    // SAFETY: loads a system DLL from System32 only; it stays loaded.
    unsafe {
        let module = LoadLibraryExW(
            w!("Windows.UI.Xaml.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
        .map_err(|e| {
            log!("could not load Windows.UI.Xaml.dll: {e}");
            e.code()
        })?;
        let Some(proc) = GetProcAddress(module, s!("InitializeXamlDiagnosticsEx")) else {
            log!("InitializeXamlDiagnosticsEx is not exported");
            return Err(E_UNEXPECTED);
        };
        // SAFETY: the documented signature of this export (xamlom.h).
        Ok(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            InitializeXamlDiagnosticsEx,
        >(proc))
    }
}

fn call_on_fresh_thread(
    initialize: InitializeXamlDiagnosticsEx,
    connection: &str,
    pid: u32,
    dll: &Path,
) -> HRESULT {
    let connection = HSTRING::from(connection);
    let dll = HSTRING::from(dll.as_os_str());
    let clsid = GUID::from_u128(TAP_CLSID);
    thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: the strings outlive the call; a null path selects
                // the system's XamlDiagnostics.dll, a null initialisation
                // string passes no data.
                unsafe {
                    initialize(
                        PCWSTR(connection.as_ptr()),
                        pid,
                        PCWSTR::null(),
                        PCWSTR(dll.as_ptr()),
                        clsid,
                        PCWSTR::null(),
                    )
                }
            })
            .join()
            .unwrap_or(E_UNEXPECTED)
    })
}

/// Cross-process attach, called by sepiida.exe after loading this DLL into
/// itself.
///
/// # Safety
/// `connection` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn SepiidaInitializeXamlDiagnostics(
    pid: u32,
    from: u32,
    connection: *mut u32,
) -> HRESULT {
    guard("SepiidaInitializeXamlDiagnostics", E_UNEXPECTED, || {
        if connection.is_null() {
            return E_POINTER;
        }
        match initialize(pid, from) {
            Ok((number, hr)) => {
                // SAFETY: checked non-null above.
                unsafe { *connection = number };
                hr
            }
            Err(hr) => hr,
        }
    })
}

/// `WH_CALLWNDPROC` hook procedure. Runs inside Explorer on the taskbar's
/// thread; starts the in-process attach when it sees `ATTACH_MESSAGE`.
///
/// # Safety
/// Called by Windows as a hook procedure: for `HC_ACTION`, `lparam` points
/// to a `CWPSTRUCT`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn SepiidaCallWndProc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    guard("SepiidaCallWndProc", (), || {
        if code != HC_ACTION as i32 || lparam.0 == 0 {
            return;
        }
        // SAFETY: for HC_ACTION, lparam points to a CWPSTRUCT.
        let message = unsafe { &*(lparam.0 as *const CWPSTRUCT) };
        if message.message == attach_message() {
            let from = u32::try_from(message.wParam.0)
                .unwrap_or(1)
                .clamp(1, MAX_CONNECTION);
            start_in_process(from);
        }
    });
    // SAFETY: always pass the message on to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn attach_message() -> u32 {
    let name = HSTRING::from(ATTACH_MESSAGE);
    // SAFETY: a valid, NUL-terminated name. Registered messages are stable
    // for the session, so every call returns the same number.
    unsafe { RegisterWindowMessageW(&name) }
}

/// Pins the DLL, then attaches from a new thread (never the UI thread,
/// which XAML must stay free to serve). One attach at a time.
fn start_in_process(from: u32) {
    if ATTACHING.swap(true, Ordering::AcqRel) {
        log!("an in-process attach is already running; ignoring another request");
        return;
    }
    log!("in-process attach requested (from connection {from})");
    if !module::pin() {
        log!("could not pin the DLL; not attaching");
        ATTACHING.store(false, Ordering::Release);
        return;
    }
    let spawned = thread::Builder::new()
        .name("sepiida-attach".into())
        .spawn(move || {
            guard("in-process attach thread", (), || {
                // SAFETY: takes no arguments.
                let pid = unsafe { GetCurrentProcessId() };
                if let Err(hr) = initialize(pid, from) {
                    log!("in-process attach failed: {hr}");
                }
            });
            ATTACHING.store(false, Ordering::Release);
        });
    if let Err(error) = spawned {
        log!("could not start the attach thread: {error}");
        ATTACHING.store(false, Ordering::Release);
    }
}
