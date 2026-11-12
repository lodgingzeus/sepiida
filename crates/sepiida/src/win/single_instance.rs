//! Makes sure only one background agent runs per user session.

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::CreateMutexW,
    },
    core::w,
};

/// Held for as long as this process is the agent.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW and is closed only here.
        // Failure to close at exit is harmless.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// `Ok(None)` when another agent is already running in this session.
pub fn acquire() -> windows::core::Result<Option<InstanceGuard>> {
    // SAFETY: the name is a static, null-terminated literal. The `Local\`
    // prefix scopes the mutex to this logon session.
    let handle = unsafe { CreateMutexW(None, false, w!(r"Local\sepiida-agent"))? };
    // SAFETY: reads this thread's last error, which CreateMutexW sets to
    // ERROR_ALREADY_EXISTS when the mutex existed.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        drop(InstanceGuard(handle));
        return Ok(None);
    }
    Ok(Some(InstanceGuard(handle)))
}
