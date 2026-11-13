//! Loads the styling DLL into Explorer through XAML Diagnostics and sends it
//! commands.
//!
//! `InitializeXamlDiagnosticsEx` (exported by `Windows.UI.Xaml.dll`) posts
//! to a diagnostics port of the target process; XAML there loads our DLL by
//! path and calls its `DllGetClassObject`. Each XAML thread has its own
//! port, `VisualDiagConnection<N>`, with N unknown in advance, so the names
//! are tried in order until one is not "not found". XAML only accepts one
//! initialisation per calling thread, so each attempt gets a fresh thread.

use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use sepiida_core::hook_protocol::{
    CONTROL_WINDOW_CLASS, Command, TAP_CLSID, connection_names, ready_event_name,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_NOT_FOUND, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM},
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            Threading::{CreateEventW, WaitForSingleObject},
        },
        UI::WindowsAndMessaging::{
            FindWindowExW, FindWindowW, GetWindowThreadProcessId, HWND_MESSAGE, PostMessageW,
        },
    },
    core::{GUID, HRESULT, HSTRING, PCWSTR, s, w},
};

/// The DLL's file name, next to the executables.
pub const HOOK_DLL: &str = "sepiida_hook.dll";
/// How long `attach` waits for the DLL to report that it is watching.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ShellHookError {
    #[error("the taskbar window was not found; is Explorer running?")]
    NoExplorer,
    #[error("{0} was not found next to sepiida.exe")]
    MissingDll(PathBuf),
    #[error("could not locate sepiida.exe")]
    OwnPath(#[source] std::io::Error),
    #[error("Windows.UI.Xaml.dll or InitializeXamlDiagnosticsEx is not available")]
    NoXamlDiagnostics(#[source] windows::core::Error),
    #[error("no XAML diagnostics port answered in Explorer (tried {0})")]
    NoConnection(u32),
    #[error("XAML Diagnostics refused to load the DLL through {connection}: {hr}")]
    Refused { connection: String, hr: HRESULT },
    #[error("could not create the ready event")]
    Event(#[source] windows::core::Error),
    #[error("the styling DLL is not attached; run `sepiida shell attach` first")]
    NotAttached,
    #[error("could not send the command")]
    Post(#[source] windows::core::Error),
}

pub struct Attached {
    pub connection: String,
    /// Whether the DLL confirmed it is watching the visual tree.
    pub ready: bool,
}

type InitializeXamlDiagnosticsEx =
    unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> HRESULT;

/// Process ID of the Explorer that owns the taskbar.
pub fn explorer_pid() -> Result<u32, ShellHookError> {
    // SAFETY: plain Win32 calls with valid arguments.
    unsafe {
        let taskbar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
            .map_err(|_| ShellHookError::NoExplorer)?;
        let mut pid = 0;
        GetWindowThreadProcessId(taskbar, Some(&mut pid));
        if pid == 0 {
            return Err(ShellHookError::NoExplorer);
        }
        Ok(pid)
    }
}

/// The hook DLL next to the running executable. Only that file is ever
/// handed to Explorer.
pub fn hook_dll_path() -> Result<PathBuf, ShellHookError> {
    let exe = std::env::current_exe().map_err(ShellHookError::OwnPath)?;
    let dll = exe.with_file_name(HOOK_DLL);
    if dll.is_absolute() && dll.is_file() {
        Ok(dll)
    } else {
        Err(ShellHookError::MissingDll(dll))
    }
}

/// Asks Explorer's XAML to load the DLL, then waits for the DLL to confirm.
pub fn attach(pid: u32, dll: &Path) -> Result<Attached, ShellHookError> {
    let ready = ReadyEvent::create(pid)?;
    let initialize = load_initialize()?;
    let dll = HSTRING::from(dll.as_os_str());
    let not_found = ERROR_NOT_FOUND.to_hresult();
    let mut tried = 0;
    for (_, connection) in connection_names() {
        tried += 1;
        let hr = call_on_fresh_thread(initialize, &connection, pid, &dll);
        if hr == not_found {
            continue;
        }
        if hr.is_err() {
            return Err(ShellHookError::Refused { connection, hr });
        }
        return Ok(Attached {
            connection,
            ready: ready.wait(READY_TIMEOUT),
        });
    }
    Err(ShellHookError::NoConnection(tried))
}

fn load_initialize() -> Result<InitializeXamlDiagnosticsEx, ShellHookError> {
    // SAFETY: loads a system DLL from System32 only. It stays loaded for the
    // life of this short-lived process.
    unsafe {
        let module = LoadLibraryExW(
            w!("Windows.UI.Xaml.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
        .map_err(ShellHookError::NoXamlDiagnostics)?;
        let proc = GetProcAddress(module, s!("InitializeXamlDiagnosticsEx")).ok_or_else(|| {
            ShellHookError::NoXamlDiagnostics(windows::core::Error::from_thread())
        })?;
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
    dll: &HSTRING,
) -> HRESULT {
    let connection = HSTRING::from(connection);
    let clsid = GUID::from_u128(TAP_CLSID);
    thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: all strings outlive the call; a null path selects
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
            .unwrap_or(windows::Win32::Foundation::E_UNEXPECTED)
    })
}

/// Posts a command to every control window the DLL created in Explorer.
/// Returns how many received it.
pub fn send(pid: u32, command: Command) -> Result<usize, ShellHookError> {
    let (message, wparam) = command.encode();
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let mut sent = 0;
    let mut after: Option<HWND> = None;
    // SAFETY: plain Win32 calls; window handles are only used to post.
    unsafe {
        while let Ok(window) = FindWindowExW(Some(HWND_MESSAGE), after, &class, PCWSTR::null()) {
            after = Some(window);
            let mut owner = 0;
            GetWindowThreadProcessId(window, Some(&mut owner));
            if owner != pid {
                continue;
            }
            PostMessageW(Some(window), message, WPARAM(wparam), LPARAM(0))
                .map_err(ShellHookError::Post)?;
            sent += 1;
        }
    }
    if sent == 0 {
        return Err(ShellHookError::NotAttached);
    }
    Ok(sent)
}

/// `Local\sepiida-hook-ready-<pid>`, set by the DLL once attached.
struct ReadyEvent(HANDLE);

impl ReadyEvent {
    fn create(pid: u32) -> Result<Self, ShellHookError> {
        let name = HSTRING::from(ready_event_name(pid));
        // SAFETY: manual-reset, initially unset, default security (only
        // this user's processes can open it).
        unsafe { CreateEventW(None, true, false, &name) }
            .map(Self)
            .map_err(ShellHookError::Event)
    }

    fn wait(&self, timeout: Duration) -> bool {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the handle is owned by `self` and valid.
        unsafe { WaitForSingleObject(self.0, millis) == WAIT_OBJECT_0 }
    }
}

impl Drop for ReadyEvent {
    fn drop(&mut self) {
        // SAFETY: owned handle, closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
