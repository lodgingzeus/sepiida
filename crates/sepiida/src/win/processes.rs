//! Finds running processes by executable name in this user's session.

use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        RemoteDesktop::ProcessIdToSessionId,
        Threading::GetCurrentProcessId,
    },
};

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
