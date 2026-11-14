//! Finds running processes by executable name in this user's session.

use std::thread;

use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        RemoteDesktop::ProcessIdToSessionId,
        Threading::{
            GetCurrentProcessId, GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW,
            WaitForSingleObject,
        },
    },
};
use windows::core::PWSTR;

/// A running process: its executable name as Windows reports it, and id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub pid: u32,
}

/// Every process in this session whose executable name matches one of
/// `names` (case-insensitive). Other users' sessions are never touched.
pub fn find_in_session(names: &[&str]) -> windows::core::Result<Vec<Process>> {
    let ours = session_of(
        // SAFETY: takes no arguments.
        unsafe { GetCurrentProcessId() },
    );
    // SAFETY: the snapshot handle is closed below; the entry is initialised
    // with its size as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Vec::new();
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let name = wide_to_string(&entry.szExeFile);
            let pid = entry.th32ProcessID;
            if names.iter().any(|n| n.eq_ignore_ascii_case(&name))
                && ours.is_some()
                && session_of(pid) == ours
            {
                found.push(Process { name, pid });
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Ok(found)
    }
}

/// Calls `on_exit` from a background thread (named `thread_name`) once
/// process `pid` has exited, or straight away if it is already gone. The
/// thread only waits on the process handle; it never polls. `on_exit` gets
/// the exit code, or `None` if it could not be read (the process was gone
/// before it could be opened, or reading it was refused).
pub fn wait_for_exit(
    pid: u32,
    thread_name: &str,
    on_exit: impl FnOnce(Option<u32>) + Send + 'static,
) {
    let Some(process) = open_to_wait(pid) else {
        on_exit(None);
        return;
    };
    let spawned = thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || {
            // Move the whole guard in, so the handle is closed on this
            // thread after the wait.
            let (process, can_query) = process;
            // SAFETY: the handle is valid until `process` drops after the
            // wait returns.
            unsafe { WaitForSingleObject(process.0, INFINITE) };
            on_exit(can_query.then(|| exit_code(&process)).flatten());
        });
    // Without the thread the exit goes unnoticed until the next apply;
    // nothing else depends on it.
    drop(spawned);
}

/// Opens `pid` to wait for it, with the right to read its exit code if
/// that is granted (the bool says whether it was).
fn open_to_wait(pid: u32) -> Option<(Owned, bool)> {
    // SAFETY: no pointers are passed.
    let both = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        )
    };
    if let Ok(process) = both {
        return Some((Owned(process), true));
    }
    // SAFETY: no pointers are passed; SYNCHRONIZE is the only right needed
    // to wait.
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok()?;
    Some((Owned(process), false))
}

/// The exit code of a process that has exited.
fn exit_code(process: &Owned) -> Option<u32> {
    let mut code = 0u32;
    // SAFETY: a valid handle with PROCESS_QUERY_LIMITED_INFORMATION and a
    // writable out value.
    unsafe { GetExitCodeProcess(process.0, &mut code) }
        .ok()
        .map(|()| code)
}

/// A process handle closed on drop.
struct Owned(HANDLE);

// SAFETY: a kernel object handle is valid in every thread of the process,
// and the guard closes it exactly once, from whichever thread drops it.
unsafe impl Send for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// The executable file name of process `pid` (e.g. `explorer.exe`), or
/// `None` if it cannot be opened (gone, or more privileged than us).
pub fn exe_name(pid: u32) -> Option<String> {
    // SAFETY: plain calls; the buffer is writable for `len` characters and
    // the handle is closed below.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        queried.ok()?;
        let path = String::from_utf16_lossy(buffer.get(..len as usize)?);
        path.rsplit('\\').next().map(str::to_owned)
    }
}

fn session_of(pid: u32) -> Option<u32> {
    let mut session = 0;
    // SAFETY: `session` is writable.
    unsafe { ProcessIdToSessionId(pid, &mut session) }
        .ok()
        .map(|()| session)
}

/// A NUL-terminated UTF-16 buffer as a string.
fn wide_to_string(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}
