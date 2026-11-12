//! Wallpaper Engine as a wallpaper source.
//!
//! Wallpaper Engine is found by looking for its running process, which also
//! answers whether it is active at all. The current wallpaper is then asked
//! for with its documented command line, `-control getWallpaper`.

use std::{
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use sepiida_core::{
    source::{SourceError, Wallpaper, WallpaperSource},
    wallpaper_engine::{parse_get_wallpaper_output, project_wallpaper},
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
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::PWSTR,
};

const PROCESS_NAMES: [&str; 2] = ["wallpaper64.exe", "wallpaper32.exe"];
/// The command only forwards a request to the running instance, so it
/// answers at once. The limit is for an instance that has stopped responding.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
/// A path is at most 32,767 UTF-16 units; this leaves room for it in UTF-8.
const MAX_OUTPUT_BYTES: u64 = 128 * 1024;
/// Longest path `QueryFullProcessImageNameW` can return, in UTF-16 units.
const MAX_IMAGE_PATH: usize = 32_768;
/// From `winbase.h`. Keeps a console window from flashing up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, thiserror::Error)]
enum EngineError {
    #[error("could not list running processes")]
    ProcessList(#[source] windows::core::Error),
    #[error("could not run `{}`", .0.display())]
    Launch(PathBuf, #[source] std::io::Error),
    #[error("Wallpaper Engine did not answer within {} seconds", COMMAND_TIMEOUT.as_secs())]
    Timeout,
    #[error("Wallpaper Engine reported `{0}`, which is not an absolute path")]
    RelativePath(String),
}

pub struct WallpaperEngine;

impl WallpaperSource for WallpaperEngine {
    fn current(&self) -> Result<Option<Wallpaper>, SourceError> {
        let Some(executable) = running_executable()? else {
            return Ok(None);
        };
        let output = get_wallpaper(&executable)?;
        let Some(reported) = parse_get_wallpaper_output(&output)? else {
            return Ok(None);
        };
        let reported = PathBuf::from(reported);
        if !reported.is_absolute() {
            return Err(EngineError::RelativePath(reported.display().to_string()).into());
        }
        Ok(Some(project_wallpaper(&reported)?))
    }
}

/// Full path of the running Wallpaper Engine, or `None` when it is not
/// running. The path of the live process is used, never a search of `PATH`.
fn running_executable() -> Result<Option<PathBuf>, EngineError> {
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
            return Ok(Some(path));
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

/// Runs `-control getWallpaper` and returns what it printed.
fn get_wallpaper(executable: &Path) -> Result<Vec<u8>, EngineError> {
    let mut child = Command::new(executable)
        .args(["-control", "getWallpaper"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| EngineError::Launch(executable.to_owned(), error))?;

    let output = read_with_timeout(&mut child);
    if output.is_none() {
        // Killing closes the pipe, which lets the reader thread finish. An
        // error here means the process has already gone.
        let _ = child.kill();
    }
    // Reap the process either way; its exit status carries no information.
    let _ = child.wait();
    output.ok_or(EngineError::Timeout)
}

fn read_with_timeout(child: &mut Child) -> Option<Vec<u8>> {
    let stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        // A read error leaves whatever arrived before it; the parser decides
        // whether that is usable.
        let _ = stdout.take(MAX_OUTPUT_BYTES).read_to_end(&mut output);
        // The receiver is gone only after a timeout, when nobody wants it.
        let _ = sender.send(output);
    });
    receiver.recv_timeout(COMMAND_TIMEOUT).ok()
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed exactly once. Nothing can be
        // done about a failure to close.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
