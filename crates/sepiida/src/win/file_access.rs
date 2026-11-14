//! Lets sandboxed (AppContainer) shell processes read the hook DLL.
//!
//! An AppContainer process can only open files whose DACL grants access to
//! `ALL APPLICATION PACKAGES` (S-1-15-2-1), or for the stricter "less
//! privileged" AppContainers `ALL RESTRICTED APPLICATION PACKAGES`
//! (S-1-15-2-2). A file the user downloaded has neither, so XAML in the
//! Start menu could not load the DLL. Read and execute is all that is
//! granted, on the DLL file and its folder only; the user owns both, so no
//! elevation is needed.

use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree, WIN32_ERROR},
        Security::{
            ACL,
            Authorization::{
                ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW,
                NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW,
                TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
            },
            DACL_SECURITY_INFORMATION, NO_INHERITANCE, PSECURITY_DESCRIPTOR, PSID,
        },
        Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ},
    },
    core::{HSTRING, PWSTR},
};

/// ALL APPLICATION PACKAGES and ALL RESTRICTED APPLICATION PACKAGES.
const APP_PACKAGE_SIDS: [&str; 2] = ["S-1-15-2-1", "S-1-15-2-2"];

#[derive(Debug, thiserror::Error)]
pub enum FileAccessError {
    #[error("could not read the permissions of {0}")]
    Read(String, #[source] windows::core::Error),
    #[error("could not build the new permissions")]
    Build(#[source] windows::core::Error),
    #[error("could not change the permissions of {0}")]
    Write(String, #[source] windows::core::Error),
    #[error("invalid SID {0}")]
    Sid(&'static str, #[source] windows::core::Error),
}

/// Makes the hook DLL loadable by sandboxed processes: grants the file
/// and its folder, since opening the file goes through the folder.
pub fn grant_app_packages_read_dll(dll: &std::path::Path) -> Result<(), FileAccessError> {
    if let Some(folder) = dll.parent() {
        grant_app_packages_read(folder)?;
    }
    grant_app_packages_read(dll)
}

/// Adds read-and-execute for app packages to `path`'s DACL, keeping every
/// existing entry. Granting again is harmless: the entries merge.
fn grant_app_packages_read(path: &std::path::Path) -> Result<(), FileAccessError> {
    let display = path.display().to_string();
    let name = HSTRING::from(path.as_os_str());
    let sids = APP_PACKAGE_SIDS
        .iter()
        .map(|&text| LocalSid::parse(text))
        .collect::<Result<Vec<_>, _>>()?;

    let mut dacl: *mut ACL = core::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the out pointers are writable; `descriptor` owns the buffer
    // `dacl` points into and is freed below.
    let status = unsafe {
        GetNamedSecurityInfoW(
            &name,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut descriptor,
        )
    };
    let descriptor = LocalBuffer(descriptor.0);
    check(status).map_err(|e| FileAccessError::Read(display.clone(), e))?;

    let entries: Vec<EXPLICIT_ACCESS_W> = sids
        .iter()
        .map(|sid| EXPLICIT_ACCESS_W {
            grfAccessPermissions: (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: core::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: PWSTR(sid.0.0.cast()),
            },
        })
        .collect();
    let mut new_dacl: *mut ACL = core::ptr::null_mut();
    // SAFETY: the entries and the old DACL are valid for the call; the new
    // DACL is allocated for us and freed below.
    let status = unsafe { SetEntriesInAclW(Some(&entries), Some(dacl), &mut new_dacl) };
    let new_dacl_buffer = LocalBuffer(new_dacl.cast());
    check(status).map_err(FileAccessError::Build)?;

    // SAFETY: a valid path and a valid DACL built above.
    let status = unsafe {
        SetNamedSecurityInfoW(
            &name,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(new_dacl),
            None,
        )
    };
    drop(new_dacl_buffer);
    drop(descriptor);
    check(status).map_err(|e| FileAccessError::Write(display, e))
}

fn check(status: WIN32_ERROR) -> windows::core::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(windows::core::Error::from(status.to_hresult()))
    }
}

/// A buffer allocated by the system with `LocalAlloc`, freed on drop.
struct LocalBuffer(*mut core::ffi::c_void);

impl Drop for LocalBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: allocated by the API that returned it, freed once.
            unsafe {
                LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
}

/// A SID parsed from its string form; the buffer frees it on drop.
struct LocalSid(
    PSID,
    #[allow(dead_code, reason = "held only to free the SID on drop")] LocalBuffer,
);

impl LocalSid {
    fn parse(text: &'static str) -> Result<Self, FileAccessError> {
        let mut sid = PSID::default();
        // SAFETY: `sid` is writable; the result is freed by `LocalBuffer`.
        unsafe { ConvertStringSidToSidW(&HSTRING::from(text), &mut sid) }
            .map_err(|e| FileAccessError::Sid(text, e))?;
        Ok(Self(sid, LocalBuffer(sid.0)))
    }
}
