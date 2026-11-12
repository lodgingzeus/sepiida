//! "Start with Windows": one value under the current user's `Run` key.
//! Per user, so no admin rights are needed.

use std::ffi::c_void;

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_UNSUPPORTED_TYPE},
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
    },
    core::HSTRING,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "sepiida";
/// A command line longer than this is not one sepiida wrote.
const MAX_COMMAND_CHARS: usize = 32_768;

#[derive(Debug, thiserror::Error)]
#[error("could not {0} the start-with-Windows entry")]
pub struct StartupError(&'static str, #[source] windows::core::Error);

/// The current entry, or `None` when there is none (or it is not a string
/// sepiida could have written).
pub fn read() -> Result<Option<String>, StartupError> {
    let mut buffer = vec![0u16; MAX_COMMAND_CHARS];
    let mut len = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: `buffer` is valid for `len` bytes and the strings outlive the
    // call. RRF_RT_REG_SZ rejects other types and null-terminates.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(VALUE_NAME),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            Some(&mut len),
        )
    };
    if [
        ERROR_FILE_NOT_FOUND,
        ERROR_UNSUPPORTED_TYPE,
        ERROR_MORE_DATA,
    ]
    .contains(&status)
    {
        return Ok(None);
    }
    status.ok().map_err(|e| StartupError("read", e))?;
    // `len` is in bytes and includes the terminating null.
    let units = usize::try_from(len / 2).unwrap_or(0).min(buffer.len());
    let text = &buffer[..units];
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Ok(String::from_utf16(&text[..end]).ok())
}

pub fn enable(command: &str) -> Result<(), StartupError> {
    let wide: Vec<u16> = command.encode_utf16().chain([0]).collect();
    let bytes = u32::try_from(wide.len() * 2)
        .map_err(|_| StartupError("write", windows::core::Error::from(ERROR_MORE_DATA)))?;
    // SAFETY: `wide` is a null-terminated UTF-16 string of `bytes` bytes,
    // and the key and name outlive the call.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(VALUE_NAME),
            REG_SZ.0,
            Some(wide.as_ptr().cast::<c_void>()),
            bytes,
        )
    };
    status.ok().map_err(|e| StartupError("write", e))
}

/// Removes the entry. An entry that is already gone is not an error.
pub fn disable() -> Result<(), StartupError> {
    // SAFETY: both strings outlive the call.
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(VALUE_NAME),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().map_err(|e| StartupError("remove", e))
}
