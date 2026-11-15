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
//!   [`SepiidaAttachWithRoles`] with Explorer's pid.
//! - In-process (TranslucentTB's current method): sepiida.exe installs
//!   [`SepiidaCallWndProc`] as a `WH_CALLWNDPROC` hook on the taskbar's
//!   thread only, which maps this DLL into Explorer, and sends the taskbar
//!   window `ATTACH_MESSAGE`. The hook procedure, now running inside
//!   Explorer, pins the DLL and calls `InitializeXamlDiagnosticsEx` for its
//!   own process from a new thread.
//!
//! File Explorer's WinUI 3 XAML is reached the in-process way only: its
//! `InitializeXamlDiagnosticsEx` lives in `Microsoft.Internal.FrameworkUdk.dll`,
//! which Explorer loads with its first folder window. sepiida.exe hooks
//! that window's thread and sends it `ATTACH_WINUI_MESSAGE`; the DLL then
//! uses the module Explorer already has loaded (never loading one by
//! itself) and the `WinUIVisualDiagConnection<N>` ports.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use sepiida_core::shell::{
    attach_request::{AttachRequest, ExportColors},
    framework::Framework,
    hosts::AttachMode,
    protocol::{ALONGSIDE_SPAN, ATTACH_MESSAGE, ATTACH_WINUI_MESSAGE, MAX_CONNECTION, TAP_CLSID},
};
use windows::{
    Win32::{
        Foundation::{
            E_INVALIDARG, E_POINTER, E_UNEXPECTED, ERROR_NOT_FOUND, HMODULE, LPARAM, LRESULT,
            WPARAM,
        },
        System::{
            LibraryLoader::{
                GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
            },
            Threading::GetCurrentProcessId,
        },
        UI::WindowsAndMessaging::{CWPSTRUCT, CallNextHookEx, HC_ACTION, RegisterWindowMessageW},
    },
    core::{GUID, HRESULT, HSTRING, PCWSTR, s, w},
};

use crate::{guard, log::log, module, session};

type InitializeXamlDiagnosticsEx =
    unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> HRESULT;

/// Set while an in-process attach is running in this process.
static ATTACHING: AtomicBool = AtomicBool::new(false);
/// Set the first time the hook procedure runs in this process, so the log
/// shows whether Windows loaded the DLL into Explorer at all.
static HOOK_SEEN: AtomicBool = AtomicBool::new(false);

/// Asks `framework`'s XAML in process `pid` to load this DLL for
/// `request`, trying connections from `from` on. Returns the connection
/// number that answered and its result.
pub fn initialize(
    pid: u32,
    from: u32,
    request: &AttachRequest,
    framework: Framework,
) -> Result<(u32, HRESULT), HRESULT> {
    let Some(path) = module::this_path() else {
        return Err(E_UNEXPECTED);
    };
    let initialize = load_initialize(framework)?;
    let not_found = ERROR_NOT_FOUND.to_hresult();
    let mut tried = 0;
    // A further core sits close to the one before; the rest of the range
    // is not worth the time.
    let span = if request.mode == AttachMode::StyleAlongside {
        ALONGSIDE_SPAN
    } else {
        usize::MAX
    };
    for (number, name) in framework.connection_names(from).take(span) {
        tried += 1;
        let hr = call_on_fresh_thread(initialize, &name, pid, &path, request, framework);
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

fn load_initialize(framework: Framework) -> Result<InitializeXamlDiagnosticsEx, HRESULT> {
    let module = diagnostics_module(framework)?;
    // SAFETY: a module handle that stays loaded; a static, NUL-terminated
    // export name.
    unsafe {
        let Some(proc) = GetProcAddress(module, s!("InitializeXamlDiagnosticsEx")) else {
            log!(
                "InitializeXamlDiagnosticsEx is not exported by {}",
                framework.diagnostics_module()
            );
            return Err(E_UNEXPECTED);
        };
        // SAFETY: the documented signature of this export (xamlom.h).
        Ok(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            InitializeXamlDiagnosticsEx,
        >(proc))
    }
}

/// System XAML's module is loaded from System32. WinUI 3's comes from the
/// Windows App SDK package Explorer uses, so only the copy the process
/// already has loaded is taken.
fn diagnostics_module(framework: Framework) -> Result<HMODULE, HRESULT> {
    let name = HSTRING::from(framework.diagnostics_module());
    // SAFETY: a valid, NUL-terminated name. A System32 load stays loaded
    // for the life of the process; a handle from `GetModuleHandleW` stays
    // valid because XAML keeps its framework loaded.
    let module = unsafe {
        match framework {
            Framework::System => LoadLibraryExW(&name, None, LOAD_LIBRARY_SEARCH_SYSTEM32),
            Framework::WinUi => GetModuleHandleW(&name),
        }
    };
    module.map_err(|e| {
        log!("{name} is not available: {e}");
        e.code()
    })
}

fn call_on_fresh_thread(
    initialize: InitializeXamlDiagnosticsEx,
    connection: &str,
    pid: u32,
    dll: &Path,
    request: &AttachRequest,
    framework: Framework,
) -> HRESULT {
    // An empty diagnostics path for WinUI 3, as Windhawk's File Explorer
    // Styler passes (the only call shape proven there); null for system
    // XAML, proven since M3. Either selects the framework's own
    // XamlDiagnostics.
    let connection = HSTRING::from(connection);
    let dll = HSTRING::from(dll.as_os_str());
    let data = request.init_data().map(HSTRING::from);
    let clsid = GUID::from_u128(TAP_CLSID);
    thread::scope(|scope| {
        scope
            .spawn(|| {
                let tool_path = match framework {
                    Framework::System => PCWSTR::null(),
                    Framework::WinUi => w!(""),
                };
                // SAFETY: the strings outlive the call; `tool_path` is null
                // or an empty, NUL-terminated string; a null initialisation
                // string passes no data (style mode).
                unsafe {
                    initialize(
                        PCWSTR(connection.as_ptr()),
                        pid,
                        tool_path,
                        PCWSTR(dll.as_ptr()),
                        clsid,
                        data.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                    )
                }
            })
            .join()
            .unwrap_or(E_UNEXPECTED)
    })
}

/// Cross-process attach, called by sepiida.exe after loading this DLL into
/// itself. `mode` is an `AttachMode` id; `colors` points at `count`
/// colours to start with, one per role, encoded as
/// `AttachRequest::export_colors`. Any other `count` is refused.
///
/// # Safety
/// `colors` must be null or point at `count` readable values;
/// `connection` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn SepiidaAttachWithRoles(
    pid: u32,
    from: u32,
    mode: u32,
    colors: *const u32,
    count: u32,
    connection: *mut u32,
) -> HRESULT {
    guard("SepiidaAttachWithRoles", E_UNEXPECTED, || {
        if connection.is_null() || colors.is_null() {
            return E_POINTER;
        }
        let mut values: ExportColors = Default::default();
        let len = values.len();
        if usize::try_from(count).ok() != Some(len) {
            return E_INVALIDARG;
        }
        // SAFETY: non-null, and the caller vouches for `count` values,
        // which was just checked to be `len`.
        values.copy_from_slice(unsafe { std::slice::from_raw_parts(colors, len) });
        let Some(request) =
            AttachMode::from_id(mode).and_then(|mode| AttachRequest::from_export(mode, values))
        else {
            return E_INVALIDARG;
        };
        match initialize(pid, from, &request, Framework::System) {
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
/// thread, or on a File Explorer window's; starts the in-process attach
/// when it sees `ATTACH_MESSAGE` (system XAML) or `ATTACH_WINUI_MESSAGE`.
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
        if !HOOK_SEEN.swap(true, Ordering::AcqRel) {
            log!("hook procedure running in this process (code {code})");
        }
        if code != HC_ACTION as i32 || lparam.0 == 0 {
            return;
        }
        // SAFETY: for HC_ACTION, lparam points to a CWPSTRUCT.
        let message = unsafe { &*(lparam.0 as *const CWPSTRUCT) };
        if message.message == registered(ATTACH_MESSAGE) {
            let from = u32::try_from(message.wParam.0)
                .unwrap_or(1)
                .clamp(1, MAX_CONNECTION);
            start_in_process(Framework::System, from, AttachRequest::default());
        } else if message.message == registered(ATTACH_WINUI_MESSAGE) {
            let mode = u32::try_from(message.wParam.0)
                .ok()
                .and_then(AttachMode::from_id);
            match mode {
                Some(mode) => {
                    start_in_process(
                        Framework::WinUi,
                        1,
                        AttachRequest::new(mode, &[]).for_framework(Framework::WinUi),
                    );
                }
                None => log!("WinUI attach message with an unknown mode; ignored"),
            }
        }
    });
    // SAFETY: always pass the message on to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn registered(message: &str) -> u32 {
    let name = HSTRING::from(message);
    // SAFETY: a valid, NUL-terminated name. Registered messages are stable
    // for the session, so every call returns the same number.
    unsafe { RegisterWindowMessageW(&name) }
}

/// Pins the DLL, then attaches from a new thread (never the UI thread,
/// which XAML must stay free to serve). One attach at a time.
fn start_in_process(framework: Framework, from: u32, request: AttachRequest) {
    if ATTACHING.swap(true, Ordering::AcqRel) {
        log!("an in-process attach is already running; ignoring another request");
        return;
    }
    log!(
        "in-process attach requested ({framework:?} XAML, {:?} mode, from connection {from})",
        request.mode
    );
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
                if let Err(hr) = initialize(pid, from, &request, framework) {
                    log!("in-process attach failed: {hr}");
                    session::signal_failed();
                }
            });
            ATTACHING.store(false, Ordering::Release);
        });
    if let Err(error) = spawned {
        log!("could not start the attach thread: {error}");
        ATTACHING.store(false, Ordering::Release);
    }
}
