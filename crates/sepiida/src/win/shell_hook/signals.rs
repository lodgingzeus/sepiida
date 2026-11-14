//! Waiting for an in-process attach: the DLL's ready and failed events and
//! the target process.

use std::time::Duration;

use sepiida_core::shell::protocol::{failed_event_name, ready_event_name};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, OpenProcess, PROCESS_SYNCHRONIZE, WaitForMultipleObjects,
        },
    },
    core::HSTRING,
};

use super::ShellHookError;

/// How waiting for the DLL ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The DLL is watching the visual tree.
    Ready,
    /// The DLL gave up: no XAML diagnostics port answered (Explorer's XAML
    /// is not up yet, or the hook never ran).
    Failed,
    /// The target process exited (Explorer restarted mid-attach).
    Exited,
    /// Nothing within the timeout.
    TimedOut,
}

/// The DLL's ready and failed events (`Local\sepiida-hook-*-<pid>`) and
/// the target process, so that waiting ends as soon as any of them says
/// how the attach went.
pub(super) struct Signals {
    ready: HANDLE,
    failed: HANDLE,
    process: Option<HANDLE>,
}

impl Signals {
    pub(super) fn create(pid: u32) -> Result<Self, ShellHookError> {
        let ready = create_event(&ready_event_name(pid))?;
        let failed = match create_event(&failed_event_name(pid)) {
            Ok(failed) => failed,
            Err(error) => {
                close(ready);
                return Err(error);
            }
        };
        // SAFETY: plain call; without the handle a process exit is only
        // noticed by the timeout.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok();
        Ok(Self {
            ready,
            failed,
            process,
        })
    }

    pub(super) fn wait(&self, timeout: Duration) -> Outcome {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let mut handles = vec![self.ready, self.failed];
        handles.extend(self.process);
        // SAFETY: the handles are owned by `self` and valid.
        let result = unsafe { WaitForMultipleObjects(&handles, false, millis) };
        match result.0.wrapping_sub(WAIT_OBJECT_0.0) {
            0 => Outcome::Ready,
            1 => Outcome::Failed,
            2 if self.process.is_some() => Outcome::Exited,
            _ => Outcome::TimedOut,
        }
    }
}

/// A manual-reset event, initially unset, with default security (only
/// this user's processes can open it).
fn create_event(name: &str) -> Result<HANDLE, ShellHookError> {
    let name = HSTRING::from(name);
    // SAFETY: plain call with a valid name.
    unsafe { CreateEventW(None, true, false, &name) }.map_err(ShellHookError::Event)
}

fn close(handle: HANDLE) {
    // SAFETY: an owned handle, closed once by the caller's contract.
    unsafe {
        let _ = CloseHandle(handle);
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        close(self.ready);
        close(self.failed);
        if let Some(process) = self.process {
            close(process);
        }
    }
}
