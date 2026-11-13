//! A small append-only log file. The DLL runs inside Explorer with no
//! console, and every Windows test round is expensive, so it records what it
//! found and every failed call.
//!
//! Logging never fails loudly: if the file cannot be opened the message is
//! dropped. Nothing here may panic.
//!
//! In a sandboxed (AppContainer) process such as the Start menu the
//! sepiida log folder is not writable, so the log goes to that process's
//! own temp folder instead (`sepiida shell dump collect` gathers it).

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use sepiida_core::shell::hosts::{
    FALLBACK_LOG_FILE, MAX_PACKAGE_FAMILY_NAME, is_package_family_name,
};
use windows::Win32::{
    Foundation::ERROR_SUCCESS,
    Storage::{FileSystem::GetTempPathW, Packaging::Appx::GetCurrentPackageFamilyName},
    System::{
        SystemInformation::GetLocalTime,
        Threading::{GetCurrentProcessId, GetCurrentThreadId},
    },
};
use windows::core::PWSTR;

const LOG_FILE: &str = "hook.log";
const OLD_LOG_FILE: &str = "hook.old.log";
/// Moved aside when the DLL first logs and the file is larger than this.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// `%LOCALAPPDATA%\sepiida\logs`, the same folder the app logs to.
fn log_dir() -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    base.is_absolute()
        .then(|| base.join("sepiida").join("logs"))
}

/// The package's own temp folder, `%LOCALAPPDATA%\Packages\<family>\AC\Temp`,
/// when this process belongs to a package: the one place an AppContainer
/// process can always write. `GetTempPathW` there may name the user's temp
/// folder (inherited environment), which it cannot write.
fn package_temp_dir() -> Option<PathBuf> {
    let mut buffer = [0u16; MAX_PACKAGE_FAMILY_NAME + 1];
    let mut len = buffer.len() as u32;
    // SAFETY: `len` holds the buffer's length in characters; the buffer is
    // writable for all of it.
    let status = unsafe { GetCurrentPackageFamilyName(&mut len, Some(PWSTR(buffer.as_mut_ptr()))) };
    if status != ERROR_SUCCESS {
        return None; // Not packaged, or no room.
    }
    let len = (len as usize).saturating_sub(1).min(buffer.len());
    let family = String::from_utf16(&buffer[..len]).ok()?;
    if !is_package_family_name(&family) {
        return None;
    }
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    local
        .is_absolute()
        .then(|| local.join("Packages").join(family).join("AC").join("Temp"))
}

/// This process's temp folder.
fn temp_dir() -> Option<PathBuf> {
    let mut buffer = [0u16; 512];
    // SAFETY: the buffer is writable for its whole length.
    let len = unsafe { GetTempPathW(Some(&mut buffer)) } as usize;
    if len == 0 || len >= buffer.len() {
        return None;
    }
    let dir = PathBuf::from(String::from_utf16_lossy(&buffer[..len]));
    dir.is_absolute().then_some(dir)
}

fn open_in(dir: &std::path::Path, file: &str, old_file: &str) -> Option<File> {
    fs::create_dir_all(dir).ok()?;
    let path = dir.join(file);
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        // Best effort; if it fails we keep appending to the big file.
        let _ = fs::rename(&path, dir.join(old_file));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn open() -> Option<File> {
    let fallback = |dir: PathBuf| open_in(&dir, FALLBACK_LOG_FILE, "sepiida-hook.old.log");
    log_dir()
        .and_then(|dir| open_in(&dir, LOG_FILE, OLD_LOG_FILE))
        .or_else(|| package_temp_dir().and_then(fallback))
        .or_else(|| temp_dir().and_then(fallback))
}

pub fn write(args: std::fmt::Arguments<'_>) {
    // A poisoned lock only means an earlier write panicked mid-way; the
    // file handle is still usable.
    let mut guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = open();
    }
    let Some(file) = guard.as_mut() else {
        return;
    };
    // SAFETY: these calls take no arguments and cannot fail.
    let (t, pid, tid) = unsafe { (GetLocalTime(), GetCurrentProcessId(), GetCurrentThreadId()) };
    // One write per line: several shell processes append to the same file,
    // and a line written in pieces gets interleaved with theirs.
    let line = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} [{pid}:{tid}] {args}\n",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    );
    let _ = file.write_all(line.as_bytes());
}

/// `format!`-style logging to the hook log.
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::write(format_args!($($arg)*))
    };
}
pub(crate) use log;
