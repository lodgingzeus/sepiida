//! Getting the DLL attached: in-process through the taskbar thread's hook,
//! or remotely through its `InitializeXamlDiagnosticsEx` export.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use sepiida_core::shell::protocol::{
    ATTACH_MESSAGE, HOOK_PROC_EXPORT, INITIALIZE_EXPORT, MAX_CONNECTION,
};
use sepiida_core::{
    Color,
    shell::{attach_request::AttachRequest, hosts::AttachMode, rules::Role},
};
use windows::{
    Win32::{
        Foundation::{ERROR_NOT_FOUND, HINSTANCE, HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            FindWindowW, GetWindowThreadProcessId, HOOKPROC, RegisterWindowMessageW,
            SendNotifyMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_CALLWNDPROC,
        },
    },
    core::{HRESULT, HSTRING, PCWSTR, w},
};

use super::{
    ShellHookError,
    control::{detach_all, wait_for_detach, wait_until_attached},
    library::Library,
    signals::{Outcome, Signals},
};
use crate::win::{
    modules,
    process_info::{self, Arch},
};

/// The DLL's file name, next to the executables.
pub const HOOK_DLL: &str = "sepiida_hook.dll";
/// How long `attach` waits for the DLL to report that it is watching.
/// Trying many connection numbers from inside Explorer takes a while.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    InProcess,
    Remote,
}

pub struct Attached {
    /// Control windows of earlier copies asked to detach first.
    pub replaced: usize,
    /// Remote only: the connection number that answered and its result.
    pub remote_result: Option<(u32, HRESULT)>,
    /// How waiting for the DLL ended.
    pub outcome: Outcome,
    /// Where Explorer has the DLL loaded from, if it has; or why that
    /// could not be checked.
    pub loaded: ModuleCheck,
    /// In-process only: the same check made while the hook was still
    /// installed. Loaded then but not after means the hook procedure never
    /// acted on the attach message.
    pub loaded_while_hooked: Option<ModuleCheck>,
    /// In-process only: removing the hook failed. The DLL has pinned
    /// itself by then, so this only matters for the log.
    pub unhook_error: Option<windows::core::Error>,
}

pub type ModuleCheck = Result<Option<PathBuf>, windows::core::Error>;

type InitializeExport = unsafe extern "system" fn(u32, u32, u32, u32, u32, *mut u32) -> HRESULT;

/// The taskbar window, and the process and thread that own it.
pub struct Taskbar {
    window: HWND,
    pub pid: u32,
    thread: u32,
}

pub fn taskbar() -> Result<Taskbar, ShellHookError> {
    // SAFETY: plain Win32 calls with valid arguments.
    unsafe {
        let window = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
            .map_err(|_| ShellHookError::NoExplorer)?;
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 || thread == 0 {
            return Err(ShellHookError::NoExplorer);
        }
        Ok(Taskbar {
            window,
            pid,
            thread,
        })
    }
}

/// The hook DLL next to the running executable. Only that file is ever
/// loaded or handed to Explorer.
pub fn hook_dll_path() -> Result<PathBuf, ShellHookError> {
    let exe = std::env::current_exe().map_err(ShellHookError::OwnPath)?;
    let dll = exe.with_file_name(HOOK_DLL);
    if dll.is_absolute() && dll.is_file() {
        Ok(dll)
    } else {
        Err(ShellHookError::MissingDll(dll))
    }
}

/// Attaches the DLL inside Explorer, trying connection numbers from `from`
/// on, and waits for it to confirm.
pub fn attach(
    taskbar: &Taskbar,
    dll: &Path,
    from: u32,
    method: Method,
) -> Result<Attached, ShellHookError> {
    check_arch(taskbar.pid)?;
    let replaced = detach_all(taskbar.pid)?;
    if replaced > 0 {
        wait_for_detach(taskbar.pid);
    }
    let mut loaded_while_hooked = None;
    let mut unhook_error = None;
    let signals = Signals::create(taskbar.pid)?;
    let library = Library::load(dll)?;
    let (remote_result, outcome) = match method {
        Method::InProcess => {
            let hooked = attach_in_process(taskbar, &library, from, &signals)?;
            loaded_while_hooked = Some(hooked.loaded);
            unhook_error = hooked.unhook_error;
            (None, hooked.outcome)
        }
        Method::Remote => {
            let result = attach_remote(taskbar.pid, &library, from, &AttachRequest::default())?;
            (Some(result), signals.wait(READY_TIMEOUT))
        }
    };
    Ok(Attached {
        replaced,
        remote_result,
        outcome,
        loaded: modules::loaded_module(taskbar.pid, HOOK_DLL),
        loaded_while_hooked,
        unhook_error,
    })
}

struct Hooked {
    outcome: Outcome,
    loaded: ModuleCheck,
    unhook_error: Option<windows::core::Error>,
}

fn attach_in_process(
    taskbar: &Taskbar,
    library: &Library,
    from: u32,
    signals: &Signals,
) -> Result<Hooked, ShellHookError> {
    let proc = library.export(HOOK_PROC_EXPORT)?;
    // SAFETY: the export has the HOOKPROC signature (see the DLL's
    // `SepiidaCallWndProc`).
    let proc: HOOKPROC = Some(unsafe {
        std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(i32, WPARAM, LPARAM) -> windows::Win32::Foundation::LRESULT,
        >(proc)
    });
    // SAFETY: the hook is scoped to the taskbar's thread only (never 0,
    // which would load the DLL into every process on the desktop), and is
    // removed before returning.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_CALLWNDPROC,
            proc,
            Some(HINSTANCE(library.0.0)),
            taskbar.thread,
        )
    }
    .map_err(ShellHookError::Hook)?;
    let name = HSTRING::from(ATTACH_MESSAGE);
    // SAFETY: plain Win32 calls; the message is ignored by the taskbar's own
    // window procedure and only acted on by our hook.
    let sent = unsafe {
        let message = RegisterWindowMessageW(&name);
        SendNotifyMessageW(taskbar.window, message, WPARAM(from as usize), LPARAM(0))
    };
    // The DLL pins itself inside Explorer before attaching, so the hook can
    // go once the DLL has reported back (or given up).
    let outcome = if sent.is_ok() {
        signals.wait(READY_TIMEOUT)
    } else {
        Outcome::Failed
    };
    let while_hooked = modules::loaded_module(taskbar.pid, HOOK_DLL);
    // SAFETY: the hook handle from above, removed once.
    let unhook_error = unsafe { UnhookWindowsHookEx(hook) }.err();
    sent.map_err(ShellHookError::Trigger)?;
    Ok(Hooked {
        outcome,
        loaded: while_hooked,
        unhook_error,
    })
}

/// Refuses a target whose architecture differs from this build's: the DLL
/// could never load there.
fn check_arch(pid: u32) -> Result<(), ShellHookError> {
    let ours = Arch::this_build();
    match process_info::process_arch(pid) {
        Some(explorer) if explorer != ours => Err(ShellHookError::WrongArch { explorer, ours }),
        _ => Ok(()),
    }
}

/// Asks XAML in process `pid` (any shell process, sandboxed or not) to load
/// the DLL in dump mode, from sepiida.exe, through every diagnostics
/// connection that answers: a process can have one XAML core per window
/// thread (ShellHost: one per flyout, it seems). Returns each connection
/// that answered and its result; empty if none did. Nothing waits for the
/// DLL: a sandboxed process cannot reach our events, and a suspended one
/// only acts once it runs again.
pub fn attach_dump(pid: u32, dll: &Path) -> Result<Vec<(u32, HRESULT)>, ShellHookError> {
    check_arch(pid)?;
    let library = Library::load(dll)?;
    let mut answered = Vec::new();
    let mut from = 1;
    while from <= MAX_CONNECTION {
        match attach_remote(
            pid,
            &library,
            from,
            &AttachRequest::new(AttachMode::Dump, &[]),
        ) {
            Ok((connection, hr)) | Err(ShellHookError::Refused { connection, hr }) => {
                answered.push((connection, hr));
                from = connection + 1;
            }
            Err(ShellHookError::NoConnection) => break,
            Err(error) => return Err(error),
        }
    }
    Ok(answered)
}

/// How a style-mode attach to a sandboxed host went.
pub struct SandboxAttached {
    /// Control windows of earlier copies asked to detach first.
    pub replaced: usize,
    /// Every connection that answered, and its result.
    pub answered: Vec<(u32, HRESULT)>,
    /// Whether the DLL's control window appeared in time; always false
    /// for a zero timeout (not waited for). A host that is suspended
    /// (Start while closed) only runs the DLL once it is shown.
    pub confirmed: bool,
}

/// Asks XAML in a sandboxed shell host (Start, quick settings, the
/// notification centre) to load the DLL in style mode, from sepiida.exe,
/// after detaching any earlier copy, through every connection that
/// answers: one attachment sees every XAML thread of one core
/// (ShellExperienceHost), but ShellHost keeps quick settings in a further
/// core behind another connection. The first attaches in style mode, the
/// rest alongside it. `colors` travel with the attach, so a host that is
/// shown later starts with them. The host cannot reach our `Local\` events, so success is judged by the DLL's
/// control window appearing within `timeout`.
pub fn attach_sandboxed(
    pid: u32,
    dll: &Path,
    colors: &[(Role, Color)],
    timeout: Duration,
) -> Result<SandboxAttached, ShellHookError> {
    check_arch(pid)?;
    let replaced = detach_all(pid)?;
    if replaced > 0 {
        wait_for_detach(pid);
    }
    let library = Library::load(dll)?;
    let mut answered = Vec::new();
    let mut from = 1;
    while from <= MAX_CONNECTION {
        let mode = if answered.is_empty() {
            AttachMode::Style
        } else {
            AttachMode::StyleAlongside
        };
        match attach_remote(pid, &library, from, &AttachRequest::new(mode, colors)) {
            Ok((connection, hr)) | Err(ShellHookError::Refused { connection, hr }) => {
                answered.push((connection, hr));
                from = connection + 1;
            }
            Err(ShellHookError::NoConnection) => break,
            Err(error) => return Err(error),
        }
    }
    if answered.is_empty() {
        return Err(ShellHookError::NoConnection);
    }
    Ok(SandboxAttached {
        replaced,
        answered,
        confirmed: !timeout.is_zero() && wait_until_attached(pid, timeout),
    })
}

fn attach_remote(
    pid: u32,
    library: &Library,
    from: u32,
    request: &AttachRequest,
) -> Result<(u32, HRESULT), ShellHookError> {
    let proc = library.export(INITIALIZE_EXPORT)?;
    // SAFETY: the export's documented signature (see the DLL's
    // `SepiidaInitializeXamlDiagnostics`).
    let initialize = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, InitializeExport>(proc)
    };
    let mut connection = 0;
    // SAFETY: `connection` is writable.
    let [surface, accent] = request.export_colors();
    let hr = unsafe {
        initialize(
            pid,
            from,
            request.mode.id(),
            surface,
            accent,
            &mut connection,
        )
    };
    if hr == ERROR_NOT_FOUND.to_hresult() {
        return Err(ShellHookError::NoConnection);
    }
    if hr.is_err() {
        return Err(ShellHookError::Refused { connection, hr });
    }
    Ok((connection, hr))
}
