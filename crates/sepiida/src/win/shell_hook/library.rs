//! The hook DLL loaded into sepiida.exe, to call its exports.

use std::path::{Path, PathBuf};

use windows::{
    Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_FLAGS, LoadLibraryExW},
    },
    core::{HSTRING, PCSTR},
};

use super::ShellHookError;

/// The hook DLL loaded into sepiida.exe (its `DllMain` does nothing).
pub(super) struct Library(pub(super) HMODULE, PathBuf);

impl Library {
    pub(super) fn load(path: &Path) -> Result<Self, ShellHookError> {
        let wide = HSTRING::from(path.as_os_str());
        // SAFETY: a full path to our own DLL, checked to exist.
        unsafe { LoadLibraryExW(&wide, None, LOAD_LIBRARY_FLAGS(0)) }
            .map(|module| Self(module, path.to_owned()))
            .map_err(|e| ShellHookError::LoadDll(path.to_owned(), e))
    }

    pub(super) fn export(
        &self,
        name: &'static str,
    ) -> Result<unsafe extern "system" fn() -> isize, ShellHookError> {
        let c_name = format!("{name}\0");
        // SAFETY: a NUL-terminated ASCII name; the module is loaded.
        unsafe { GetProcAddress(self.0, PCSTR(c_name.as_ptr())) }
            .ok_or_else(|| ShellHookError::MissingExport(self.1.clone(), name))
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: loaded once in `load`, freed once. No hook from it is left
        // installed by then.
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
