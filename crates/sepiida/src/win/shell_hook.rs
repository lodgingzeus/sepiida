//! Gets the styling DLL attached inside Explorer and sends it commands.
//!
//! The XAML Diagnostics call itself lives in the DLL (see its `inject`
//! module), so both ways of attaching use the same code. sepiida.exe loads
//! the DLL into itself and either:
//! - in-process (default, TranslucentTB's current method): installs the
//!   DLL's `WH_CALLWNDPROC` hook on the taskbar's thread only, which maps the
//!   DLL into Explorer, and sends the taskbar window the attach message; the
//!   DLL then attaches from inside Explorer; or
//! - remote: calls the DLL's export with Explorer's pid, so the XAML
//!   Diagnostics request comes from sepiida.exe.
//!
//! Either way the DLL sets a named event once it watches the visual tree.
//!
//! Only one copy of the DLL may style a process: attaching first asks every
//! copy already there (any version, any folder) to restore and detach.

use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use sepiida_core::shell::protocol::{
    ATTACH_MESSAGE, CONTROL_WINDOW_CLASS, Command, HOOK_PROC_EXPORT, INITIALIZE_EXPORT,
    control_window_title, ready_event_name,
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_NOT_FOUND, FreeLibrary, HANDLE, HINSTANCE, HMODULE, HWND, LPARAM,
            WAIT_OBJECT_0, WPARAM,
        },
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_FLAGS, LoadLibraryExW},
            Threading::{CreateEventW, WaitForSingleObject},
        },
        UI::WindowsAndMessaging::{
            FindWindowExW, FindWindowW, GetWindowThreadProcessId, HOOKPROC, HWND_MESSAGE,
            PostMessageW, RegisterWindowMessageW, SendNotifyMessageW, SetWindowsHookExW,
            UnhookWindowsHookEx, WH_CALLWNDPROC,
        },
    },
    core::{HRESULT, HSTRING, PCSTR, PCWSTR, w},
};

use super::{
    modules,
    process_info::{self, Arch},
};

/// The DLL's file name, next to the executables.
pub const HOOK_DLL: &str = "sepiida_hook.dll";
/// How long `attach` waits for the DLL to report that it is watching.
/// Trying many connection numbers from inside Explorer takes a while.
const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// How long `attach` waits for older copies to detach before going on.
const DETACH_TIMEOUT: Duration = Duration::from_secs(2);
const DETACH_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum ShellHookError {
    #[error("the taskbar window was not found; is Explorer running?")]
    NoExplorer,
    #[error("{0} was not found next to sepiida.exe")]
    MissingDll(PathBuf),
    #[error("could not locate sepiida.exe")]
    OwnPath(#[source] std::io::Error),
    #[error("could not load {0}")]
    LoadDll(PathBuf, #[source] windows::core::Error),
    #[error("{0} does not export {1}; is it from the same build as sepiida.exe?")]
    MissingExport(PathBuf, &'static str),
    #[error("could not install the hook on the taskbar thread")]
    Hook(#[source] windows::core::Error),
    #[error("could not send the attach message to the taskbar")]
    Trigger(#[source] windows::core::Error),
    #[error(
        "Explorer runs as {} but this build is {}: a DLL can only be loaded into a process \
         of its own architecture. Use the sepiida-windows-{} build.",
        explorer.artifact_suffix(), ours.artifact_suffix(), explorer.artifact_suffix()
    )]
    WrongArch { explorer: Arch, ours: Arch },
    #[error("no XAML diagnostics port answered in Explorer")]
    NoConnection,
    #[error("XAML Diagnostics refused through VisualDiagConnection{connection}: {hr}")]
    Refused { connection: u32, hr: HRESULT },
    #[error("could not create the ready event")]
    Event(#[source] windows::core::Error),
    #[error("the styling DLL is not attached; run `sepiida shell attach` first")]
    NotAttached,
    #[error("could not send the command")]
    Post(#[source] windows::core::Error),
}

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
    /// Whether the DLL confirmed it is watching the visual tree.
    pub ready: bool,
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

type InitializeExport = unsafe extern "system" fn(u32, u32, *mut u32) -> HRESULT;

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
    let ours = Arch::this_build();
    if let Some(explorer) = process_info::process_arch(taskbar.pid)
        && explorer != ours
    {
        return Err(ShellHookError::WrongArch { explorer, ours });
    }
    let replaced = detach_all(taskbar.pid)?;
    if replaced > 0 {
        wait_for_detach(taskbar.pid);
    }
    let mut loaded_while_hooked = None;
    let mut unhook_error = None;
    let ready = ReadyEvent::create(taskbar.pid)?;
    let library = Library::load(dll)?;
    let (remote_result, ready) = match method {
        Method::InProcess => {
            let hooked = attach_in_process(taskbar, &library, from, &ready)?;
            loaded_while_hooked = Some(hooked.loaded);
            unhook_error = hooked.unhook_error;
            (None, hooked.confirmed)
        }
        Method::Remote => {
            let result = attach_remote(taskbar.pid, &library, from)?;
            (Some(result), ready.wait(READY_TIMEOUT))
        }
    };
    Ok(Attached {
        replaced,
        remote_result,
        ready,
        loaded: modules::loaded_module(taskbar.pid, HOOK_DLL),
        loaded_while_hooked,
        unhook_error,
    })
}

struct Hooked {
    confirmed: bool,
    loaded: ModuleCheck,
    unhook_error: Option<windows::core::Error>,
}

fn attach_in_process(
    taskbar: &Taskbar,
    library: &Library,
    from: u32,
    ready: &ReadyEvent,
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
    let confirmed = sent.is_ok() && ready.wait(READY_TIMEOUT);
    let while_hooked = modules::loaded_module(taskbar.pid, HOOK_DLL);
    // SAFETY: the hook handle from above, removed once.
    let unhook_error = unsafe { UnhookWindowsHookEx(hook) }.err();
    sent.map_err(ShellHookError::Trigger)?;
    Ok(Hooked {
        confirmed,
        loaded: while_hooked,
        unhook_error,
    })
}

fn attach_remote(pid: u32, library: &Library, from: u32) -> Result<(u32, HRESULT), ShellHookError> {
    let proc = library.export(INITIALIZE_EXPORT)?;
    // SAFETY: the export's documented signature (see the DLL's
    // `SepiidaInitializeXamlDiagnostics`).
    let initialize = unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, InitializeExport>(proc)
    };
    let mut connection = 0;
    // SAFETY: `connection` is writable.
    let hr = unsafe { initialize(pid, from, &mut connection) };
    if hr == ERROR_NOT_FOUND.to_hresult() {
        return Err(ShellHookError::NoConnection);
    }
    if hr.is_err() {
        return Err(ShellHookError::Refused { connection, hr });
    }
    Ok((connection, hr))
}

/// Posts a command to every control window of this protocol version in
/// process `pid`. Returns how many received it.
pub fn send(pid: u32, command: Command) -> Result<usize, ShellHookError> {
    let title = control_window_title();
    let sent = post_to_windows(pid, Some(&title), command)?;
    if sent == 0 {
        return Err(ShellHookError::NotAttached);
    }
    Ok(sent)
}

/// Whether a copy of the DLL speaking this protocol version is attached in
/// process `pid`.
pub fn is_attached(pid: u32) -> bool {
    !control_windows(pid, Some(&control_window_title())).is_empty()
}

/// Asks every copy of the DLL in process `pid`, whatever its version, to
/// restore and detach. Returns how many control windows were asked.
pub fn detach_all(pid: u32) -> Result<usize, ShellHookError> {
    post_to_windows(pid, None, Command::Detach)
}

/// Waits a little for detached copies to destroy their control windows,
/// so their restore runs before a new copy starts styling.
fn wait_for_detach(pid: u32) {
    let deadline = Instant::now() + DETACH_TIMEOUT;
    while !control_windows(pid, None).is_empty() && Instant::now() < deadline {
        thread::sleep(DETACH_POLL);
    }
}

fn post_to_windows(
    pid: u32,
    title: Option<&str>,
    command: Command,
) -> Result<usize, ShellHookError> {
    let (message, wparam) = command.encode();
    let windows = control_windows(pid, title);
    for &window in &windows {
        // SAFETY: plain Win32 call; the handle is only used to post.
        unsafe { PostMessageW(Some(window), message, WPARAM(wparam), LPARAM(0)) }
            .map_err(ShellHookError::Post)?;
    }
    Ok(windows.len())
}

/// The DLL's message-only control windows owned by process `pid`; with a
/// title, only those of that protocol version.
fn control_windows(pid: u32, title: Option<&str>) -> Vec<HWND> {
    let class = HSTRING::from(CONTROL_WINDOW_CLASS);
    let title = title.map(HSTRING::from);
    let title = title
        .as_ref()
        .map_or(PCWSTR::null(), |t| PCWSTR(t.as_ptr()));
    let mut found = Vec::new();
    let mut after: Option<HWND> = None;
    // SAFETY: plain Win32 calls; the strings outlive the loop.
    unsafe {
        while let Ok(window) = FindWindowExW(Some(HWND_MESSAGE), after, &class, title) {
            after = Some(window);
            let mut owner = 0;
            GetWindowThreadProcessId(window, Some(&mut owner));
            if owner == pid {
                found.push(window);
            }
        }
    }
    found
}

/// The hook DLL loaded into sepiida.exe (its `DllMain` does nothing).
struct Library(HMODULE, PathBuf);

impl Library {
    fn load(path: &Path) -> Result<Self, ShellHookError> {
        let wide = HSTRING::from(path.as_os_str());
        // SAFETY: a full path to our own DLL, checked to exist.
        unsafe { LoadLibraryExW(&wide, None, LOAD_LIBRARY_FLAGS(0)) }
            .map(|module| Self(module, path.to_owned()))
            .map_err(|e| ShellHookError::LoadDll(path.to_owned(), e))
    }

    fn export(
        &self,
        name: &'static str,
    ) -> Result<unsafe extern "system" fn() -> isize, ShellHookError> {
        let c_name = format!("{name}\0");
        // SAFETY: a NUL-terminated ASCII name; the module is loaded.
        unsafe { GetProcAddress(self.0, PCSTR(c_name.as_ptr())) }
            .ok_or_else(|| ShellHookError::MissingExport(self.1.clone(), name))
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: loaded once in `load`, freed once. No hook from it is left
        // installed by then.
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
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
