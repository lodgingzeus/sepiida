//! Typed reads and writes of `HKEY_CURRENT_USER` values.

use std::ffi::c_void;

use sepiida_core::{
    accent::{RegistryData, RegistryWrite},
    snapshot::Original,
};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_UNSUPPORTED_TYPE, WIN32_ERROR},
        System::Registry::{
            HKEY_CURRENT_USER, REG_BINARY, REG_DWORD, REG_ROUTINE_FLAGS, REG_VALUE_TYPE,
            RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
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

/// What a lookup of one value found.
enum Lookup {
    /// The value exists with the expected type; this many bytes were read.
    Found(usize),
    Missing,
    /// The value exists but has another type or does not fit the buffer.
    Unreadable,
}

/// Returns `None` when the value is missing, has another type, or does not
/// fit: in each case it is not the value we would write.
pub fn read(write: &RegistryWrite) -> Result<Option<RegistryData>, RegistryError> {
    match read_original(write)? {
        Original::Value(data) => Ok(Some(data)),
        Original::Absent | Original::Unknown => Ok(None),
    }
}

/// Reads the current value in the form needed to put it back later.
pub fn read_original(write: &RegistryWrite) -> Result<Original, RegistryError> {
    let (flags, capacity) = match &write.data {
        RegistryData::Dword(_) => (RRF_RT_REG_DWORD, size_of::<u32>()),
        RegistryData::Binary(_) => (RRF_RT_REG_BINARY, MAX_BINARY_LEN),
    };
    let mut buffer = [0u8; MAX_BINARY_LEN];
    let len = match get_value(write, flags, &mut buffer[..capacity])? {
        Lookup::Found(len) => len,
        Lookup::Missing => return Ok(Original::Absent),
        Lookup::Unreadable => return Ok(Original::Unknown),
    };
    let bytes = &buffer[..len];
    Ok(match &write.data {
        RegistryData::Dword(_) => <[u8; 4]>::try_from(bytes)
            .map(|b| Original::Value(RegistryData::Dword(u32::from_le_bytes(b))))
            .unwrap_or(Original::Unknown),
        RegistryData::Binary(_) => Original::Value(RegistryData::Binary(bytes.to_vec())),
    })
}

fn get_value(
    write: &RegistryWrite,
    flags: REG_ROUTINE_FLAGS,
    buffer: &mut [u8],
) -> Result<Lookup, RegistryError> {
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
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(Lookup::Missing);
    }
    if [ERROR_UNSUPPORTED_TYPE, ERROR_MORE_DATA].contains(&status) {
        return Ok(Lookup::Unreadable);
    }
    check(status, "read", write.key, write.name)?;
    // The API never reports more than it was given room for on success, but
    // the length indexes a buffer, so it is checked rather than trusted.
    Ok(usize::try_from(len)
        .ok()
        .filter(|len| *len <= buffer.len())
        .map_or(Lookup::Unreadable, Lookup::Found))
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
    check(status, "write", write.key, write.name)
}

/// Deletes a value. A value that is already gone is not an error.
pub fn delete(key: &'static str, name: &'static str) -> Result<(), RegistryError> {
    // SAFETY: both strings outlive the call.
    let status =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name)) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(status, "delete", key, name)
}

fn check(
    status: WIN32_ERROR,
    action: &'static str,
    key: &'static str,
    name: &'static str,
) -> Result<(), RegistryError> {
    status.ok().map_err(|source| RegistryError {
        action,
        key,
        name,
        source,
    })
}
