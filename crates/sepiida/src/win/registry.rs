//! Typed reads and writes of `HKEY_CURRENT_USER` values.

use std::ffi::c_void;

use sepiida_core::accent::{RegistryData, RegistryWrite};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_UNSUPPORTED_TYPE, WIN32_ERROR},
        System::Registry::{
            HKEY_CURRENT_USER, REG_BINARY, REG_DWORD, REG_ROUTINE_FLAGS, REG_VALUE_TYPE,
            RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
        },
    },
    core::HSTRING,
};

/// Larger than any binary value sepiida compares against. A stored value
/// that does not fit is simply treated as different.
const MAX_BINARY_LEN: usize = 256;

#[derive(Debug, thiserror::Error)]
#[error("could not {action} registry value HKCU\\{key}\\{name}")]
pub struct RegistryError {
    action: &'static str,
    key: &'static str,
    name: &'static str,
    #[source]
    source: windows::core::Error,
}

/// Returns `None` when the value is missing, has another type, or does not
/// fit: in each case it is not the value we would write.
pub fn read(write: &RegistryWrite) -> Result<Option<RegistryData>, RegistryError> {
    let (flags, capacity) = match &write.data {
        RegistryData::Dword(_) => (RRF_RT_REG_DWORD, size_of::<u32>()),
        RegistryData::Binary(_) => (RRF_RT_REG_BINARY, MAX_BINARY_LEN),
    };
    let mut buffer = [0u8; MAX_BINARY_LEN];
    let Some(len) = get_value(write, flags, &mut buffer[..capacity])? else {
        return Ok(None);
    };
    let bytes = &buffer[..len];
    Ok(match &write.data {
        RegistryData::Dword(_) => <[u8; 4]>::try_from(bytes)
            .ok()
            .map(|b| RegistryData::Dword(u32::from_le_bytes(b))),
        RegistryData::Binary(_) => Some(RegistryData::Binary(bytes.to_vec())),
    })
}

fn get_value(
    write: &RegistryWrite,
    flags: REG_ROUTINE_FLAGS,
    buffer: &mut [u8],
) -> Result<Option<usize>, RegistryError> {
    let mut len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `buffer` is valid for `len` bytes and both strings outlive the
    // call. The flags make the API reject any value of another type.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(write.key),
            &HSTRING::from(write.name),
            flags,
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
    check(status, "read", write)?;
    // The API never reports more than it was given room for on success, but
    // the length indexes a buffer, so it is checked rather than trusted.
    Ok(usize::try_from(len).ok().filter(|len| *len <= buffer.len()))
}

/// Creates the key if it does not exist yet.
pub fn set(write: &RegistryWrite) -> Result<(), RegistryError> {
    let dword_bytes;
    let (kind, bytes): (REG_VALUE_TYPE, &[u8]) = match &write.data {
        RegistryData::Dword(value) => {
            dword_bytes = value.to_le_bytes();
            (REG_DWORD, &dword_bytes)
        }
        RegistryData::Binary(bytes) => (REG_BINARY, bytes),
    };
    let len = u32::try_from(bytes.len()).map_err(|_| RegistryError {
        action: "write",
        key: write.key,
        name: write.name,
        source: windows::core::Error::from(ERROR_MORE_DATA),
    })?;
    // SAFETY: `bytes` is valid for `len` bytes and both strings outlive the
    // call.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(write.key),
            &HSTRING::from(write.name),
            kind.0,
            Some(bytes.as_ptr().cast::<c_void>()),
            len,
        )
    };
    check(status, "write", write)
}

fn check(
    status: WIN32_ERROR,
    action: &'static str,
    write: &RegistryWrite,
) -> Result<(), RegistryError> {
    status.ok().map_err(|source| RegistryError {
        action,
        key: write.key,
        name: write.name,
        source,
    })
}
