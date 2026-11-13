//! A small append-only log file. The DLL runs inside Explorer with no
//! console, and every Windows test round is expensive, so it records what it
//! found and every failed call.
//!
//! Logging never fails loudly: if the file cannot be opened the message is
//! dropped. Nothing here may panic.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

use windows::Win32::System::{
    SystemInformation::GetLocalTime,
    Threading::{GetCurrentProcessId, GetCurrentThreadId},
};

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

fn open() -> Option<File> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(LOG_FILE);
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        // Best effort; if it fails we keep appending to the big file.
        let _ = fs::rename(&path, dir.join(OLD_LOG_FILE));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
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
    let _ = writeln!(
        file,
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} [{pid}:{tid}] {args}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    );
}

/// `format!`-style logging to the hook log.
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::write(format_args!($($arg)*))
    };
}
pub(crate) use log;
