//! Wallpaper Engine as a wallpaper source.
//!
//! Wallpaper Engine is found by looking for its running process, which also
//! answers whether it is active at all. The current wallpaper is then read
//! from `config.json` in its install folder: `-control getWallpaper` does not
//! write to a pipe (see `sepiida_core::engine_config`), and reading a file
//! means sepiida never starts another program.

use std::{path::PathBuf, thread};

use sepiida_core::{
    engine_config::read_selected_wallpaper,
    engine_tracker::Observed,
    source::{SourceError, Wallpaper, WallpaperSource},
    wallpaper_engine::project_wallpaper,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                INFINITE, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
            },
        },
    },
    core::PWSTR,
};

const PROCESS_NAMES: [&str; 2] = ["wallpaper64.exe", "wallpaper32.exe"];
/// Longest path `QueryFullProcessImageNameW` can return, in UTF-16 units.
const MAX_IMAGE_PATH: usize = 32_768;

#[derive(Debug, thiserror::Error)]
enum EngineError {
    #[error("could not list running processes")]
    ProcessList(#[source] windows::core::Error),
    #[error("`{}` has no install folder", .0.display())]
    NoFolder(PathBuf),
    #[error("Wallpaper Engine reported `{0}`, which is not an absolute path")]
    RelativePath(String),
}

pub struct WallpaperEngine;

impl WallpaperSource for WallpaperEngine {
    fn current(&self) -> Result<Option<Wallpaper>, SourceError> {
        let Some((_, executable)) = running_executable()? else {
            return Ok(None);
        };
        let folder = executable
            .parent()
            .ok_or_else(|| EngineError::NoFolder(executable.clone()))?;
        let Some(reported) = read_selected_wallpaper(folder)? else {
            return Ok(None);
        };
        let reported = PathBuf::from(reported);
        if !reported.is_absolute() {
            return Err(EngineError::RelativePath(reported.display().to_string()).into());
        }
        Ok(Some(project_wallpaper(&reported)?))
    }
}

/// The running Wallpaper Engine's process and install folder, or `None`
/// when it is not running.
pub fn running() -> Result<Option<Observed>, SourceError> {
    Ok(running_executable()?.and_then(|(process_id, executable)| {
        Some(Observed {
            process_id,
            folder: executable.parent()?.to_owned(),
        })
    }))
}

/// Calls `on_exit` from a background thread once the process has exited,
/// or straight away if it is already gone. The thread only waits on the
/// process handle; it never polls.
pub fn wait_for_exit(process_id: u32, on_exit: impl FnOnce() + Send + 'static) {
    // SAFETY: no pointers are passed; the handle is owned by the guard.
    // SYNCHRONIZE is the only right needed to wait.
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process_id) }.map(OwnedHandle);
    let Ok(process) = process else {
        on_exit();
        return;
    };
    let spawned = thread::Builder::new()
        .name("wallpaper-engine-exit".to_owned())
        .spawn(move || {
            // Move the whole guard in, not just the handle field, so the
            // handle is closed on this thread after the wait.
            let process = process;
            // SAFETY: the handle is valid until `process` drops after the
            // wait returns.
            unsafe { WaitForSingleObject(process.0, INFINITE) };
            on_exit();
        });
    // Without the thread the exit goes unnoticed until the next apply;
    // nothing else depends on it.
    drop(spawned);
}

/// Process id and full path of the running Wallpaper Engine, or `None` when
/// it is not running. The path of the live process is used, never a search
/// of `PATH`.
fn running_executable() -> Result<Option<(u32, PathBuf)>, EngineError> {
    // SAFETY: no pointers are passed; the handle is owned by the guard.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map(OwnedHandle)
        .map_err(EngineError::ProcessList)?;

    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `entry` is a valid, correctly sized structure for both calls.
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        if is_wallpaper_engine(&entry.szExeFile)
            && let Some(path) = image_path(entry.th32ProcessID)
        {
            return Ok(Some((entry.th32ProcessID, path)));
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(None)
}

fn is_wallpaper_engine(exe_file: &[u16]) -> bool {
    let len = exe_file
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(exe_file.len());
    let name = String::from_utf16_lossy(&exe_file[..len]);
    PROCESS_NAMES
        .iter()
        .any(|expected| name.eq_ignore_ascii_case(expected))
}

/// `None` when the process has exited meanwhile or may not be queried.
fn image_path(process_id: u32) -> Option<PathBuf> {
    // SAFETY: no pointers are passed; the handle is owned by the guard.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map(OwnedHandle)
        .ok()?;
    let mut buffer = vec![0u16; MAX_IMAGE_PATH];
    let mut len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is valid for `len` UTF-16 units; on success `len` is
    // set to the number of units written.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    let written = buffer.get(..usize::try_from(len).ok()?)?;
    let path = PathBuf::from(String::from_utf16(written).ok()?);
    (path.is_absolute() && path.is_file()).then_some(path)
}

struct OwnedHandle(HANDLE);

// SAFETY: a kernel object handle is valid in every thread of the process,
// and the guard closes it exactly once, from whichever thread drops it.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed exactly once. Nothing can be
        // done about a failure to close.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
