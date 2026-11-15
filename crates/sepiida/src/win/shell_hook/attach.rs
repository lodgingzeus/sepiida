//! Getting the DLL attached: in-process through the taskbar thread's hook,
//! or remotely through its `InitializeXamlDiagnosticsEx` export; and to File
//! Explorer's WinUI 3 XAML through a folder window thread's hook.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use sepiida_core::shell::protocol::{
    ATTACH_MESSAGE, ATTACH_WINUI_MESSAGE, HOOK_PROC_EXPORT, INITIALIZE_EXPORT, MAX_CONNECTION,
};
use sepiida_core::{
    Color,
    shell::{attach_request::AttachRequest, framework::Framework, hosts::AttachMode, rules::Role},
};
use windows::{
    Win32::{
        Foundation::{ERROR_NOT_FOUND, HINSTANCE, HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            FindWindowExW, FindWindowW, GetWindowThreadProcessId, HOOKPROC, RegisterWindowMessageW,
            SendNotifyMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_CALLWNDPROC,
        },
    },
    core::{HRESULT, HSTRING, PCWSTR, w},
};

use super::{
    ShellHookError,
    control::{detach_all, detach_framework, wait_for_detach, wait_until_attached},
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
/// The same for File Explorer's WinUI 3 XAML, whose port numbers run much
/// higher (`Framework::WinUi.max_connection()`).
const WINUI_READY_TIMEOUT: Duration = Duration::from_secs(90);

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

type InitializeExport =
    unsafe extern "system" fn(u32, u32, u32, *const u32, u32, *mut u32) -> HRESULT;

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

/// A File Explorer folder window, and the process and thread that own it.
/// Folder windows may run in their own explorer.exe ("Launch folder
/// windows in a separate process"), so the pid may not be the taskbar's.
pub struct ExplorerWindow {
    window: HWND,
    pub pid: u32,
    thread: u32,
}

/// The first open File Explorer window with the modern (XAML) frame. A
/// `CabinetWClass` window with a `WorkerW` child is the legacy frame
/// (Control Panel), which has no XAML.
pub fn explorer_window() -> Result<ExplorerWindow, ShellHookError> {
    explorer_windows()
        .into_iter()
        .next()
        .ok_or(ShellHookError::NoExplorerWindow)
}

/// The first modern-frame folder window of process `pid`.
pub fn explorer_window_of(pid: u32) -> Result<ExplorerWindow, ShellHookError> {
    explorer_windows()
        .into_iter()
        .find(|w| w.pid == pid)
        .ok_or(ShellHookError::NoExplorerWindow)
}

/// The processes that have a modern-frame folder window, each once.
pub fn explorer_window_pids() -> Vec<u32> {
    let mut pids: Vec<u32> = explorer_windows().iter().map(|w| w.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// Every open modern-frame folder window, in z-order.
fn explorer_windows() -> Vec<ExplorerWindow> {
    let mut found = Vec::new();
    let mut after = None;
    // SAFETY: plain Win32 calls with valid arguments; the walk ends when
    // no further window of the class exists.
    unsafe {
        while let Ok(window) = FindWindowExW(None, after, w!("CabinetWClass"), PCWSTR::null()) {
            after = Some(window);
            if FindWindowExW(Some(window), None, w!("WorkerW"), PCWSTR::null()).is_ok() {
                continue;
            }
            let mut pid = 0;
            let thread = GetWindowThreadProcessId(window, Some(&mut pid));
            if pid != 0 && thread != 0 {
                found.push(ExplorerWindow {
                    window,
                    pid,
                    thread,
                });
            }
        }
    }
    found
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
    // File Explorer's WinUI 3 copy in the same process stays.
    let replaced = detach_framework(taskbar.pid, Framework::System)?;
    if replaced > 0 {
        wait_for_detach(taskbar.pid, Framework::System);
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

/// Attaches the DLL to File Explorer's WinUI 3 XAML in `mode`, from inside
/// the window's process, and waits for it to confirm. Windhawk attaches
/// once per process and so expects one WinUI 3 port to cover every folder
/// window (not yet confirmed here). The taskbar's copy in the same process
/// stays; a style attach first detaches earlier WinUI 3 copies, a dump
/// sits beside everything. Returns how many copies were replaced, and how
/// waiting ended.
pub fn attach_winui(
    window: &ExplorerWindow,
    dll: &Path,
    mode: AttachMode,
) -> Result<(usize, Outcome), ShellHookError> {
    check_arch(window.pid)?;
    let mut replaced = 0;
    if mode != AttachMode::Dump {
        replaced = detach_framework(window.pid, Framework::WinUi)?;
        if replaced > 0 {
            wait_for_detach(window.pid, Framework::WinUi);
        }
    }
    let signals = Signals::create(window.pid)?;
    let library = Library::load(dll)?;
    let hooked = hook_and_send(
        &library,
        window.thread,
        window.window,
        ATTACH_WINUI_MESSAGE,
        mode.id(),
        &signals,
        WINUI_READY_TIMEOUT,
    )?;
    Ok((replaced, hooked.outcome))
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
    hook_and_send(
        library,
        taskbar.thread,
        taskbar.window,
        ATTACH_MESSAGE,
        from,
        signals,
        READY_TIMEOUT,
    )
}

/// Installs the DLL's hook on `thread` only, sends `window` the registered
/// `message` with `wparam`, waits up to `timeout` for the DLL's signal,
/// and removes the hook again.
fn hook_and_send(
    library: &Library,
    thread: u32,
    window: HWND,
    message: &str,
    wparam: u32,
    signals: &Signals,
    timeout: Duration,
) -> Result<Hooked, ShellHookError> {
    let pid = {
        let mut pid = 0;
        // SAFETY: plain Win32 call with a writable out pointer.
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        pid
    };
    let proc = library.export(HOOK_PROC_EXPORT)?;
    // SAFETY: the export has the HOOKPROC signature (see the DLL's
    // `SepiidaCallWndProc`).
    let proc: HOOKPROC = Some(unsafe {
        std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(i32, WPARAM, LPARAM) -> windows::Win32::Foundation::LRESULT,
        >(proc)
    });
    if thread == 0 {
        // 0 would hook every thread on the desktop.
        return Err(ShellHookError::NoExplorer);
    }
    // SAFETY: the hook is scoped to one thread of the target (never 0, see
    // above), and is removed before returning.
    let hook =
        unsafe { SetWindowsHookExW(WH_CALLWNDPROC, proc, Some(HINSTANCE(library.0.0)), thread) }
            .map_err(ShellHookError::Hook)?;
    let name = HSTRING::from(message);
    // SAFETY: plain Win32 calls; the message is ignored by the window's own
    // procedure and only acted on by our hook.
    let sent = unsafe {
        let message = RegisterWindowMessageW(&name);
        SendNotifyMessageW(window, message, WPARAM(wparam as usize), LPARAM(0))
    };
    // The DLL pins itself inside Explorer before attaching, so the hook can
    // go once the DLL has reported back (or given up).
    let outcome = if sent.is_ok() {
        signals.wait(timeout)
    } else {
        Outcome::Failed
    };
    let while_hooked = modules::loaded_module(pid, HOOK_DLL);
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
/// answers: the first in style mode, the rest alongside it. (On build
/// 26100 the first alone covered every XAML thread; the others are a
/// hedge for builds where a host keeps a separate core.) `colors` travel
/// with the attach, so a host that is shown later starts with them. The
/// host cannot reach our `Local\` events, so success is judged by the
/// DLL's control window appearing within `timeout`.
pub fn attach_sandboxed(
    pid: u32,
    dll: &Path,
    colors: &[(Role, Color)],
    timeout: Duration,
) -> Result<SandboxAttached, ShellHookError> {
    check_arch(pid)?;
    let replaced = detach_all(pid)?;
    if replaced > 0 {
        wait_for_detach(pid, Framework::System);
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
        confirmed: !timeout.is_zero() && wait_until_attached(pid, Framework::System, timeout),
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
    // `SepiidaAttachWithRoles`).
    let initialize = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, InitializeExport>(proc)
    };
    let mut connection = 0;
    let colors = request.export_colors();
    let count = u32::try_from(colors.len()).unwrap_or(u32::MAX);
    // SAFETY: `colors` holds `count` values and outlives the call;
    // `connection` is writable.
    let hr = unsafe {
        initialize(
            pid,
            from,
            request.mode.id(),
            colors.as_ptr(),
            count,
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
