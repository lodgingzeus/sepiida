//! The normal Windows desktop wallpaper, read through `IDesktopWallpaper`.

use std::{ffi::c_void, path::PathBuf};

use sepiida_core::{
    accent::from_colorref,
    source::{Showing, SourceError, Wallpaper, WallpaperSource, windows_wallpaper},
};
use windows::{
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{
            CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
        UI::Shell::{DesktopWallpaper, IDesktopWallpaper},
    },
    core::{PCWSTR, PWSTR},
};

/// Where Windows keeps a copy of the current wallpaper, below `%APPDATA%`.
const TRANSCODED_WALLPAPER: &str = r"Microsoft\Windows\Themes\TranscodedWallpaper";

#[derive(Debug, thiserror::Error)]
enum DesktopError {
    #[error("could not initialise COM")]
    ComInit(#[source] windows::core::Error),
    #[error("could not query the desktop wallpaper")]
    Query(#[from] windows::core::Error),
    #[error("Windows reported a wallpaper path that is not valid text")]
    InvalidPath,
    #[error("the wallpaper file `{0}` does not exist")]
    MissingFile(String),
}

pub struct WindowsWallpaper;

impl WallpaperSource for WindowsWallpaper {
    fn current(&self) -> Result<Showing, SourceError> {
        Ok(Showing::Wallpaper(read_wallpaper()?))
    }
}

fn read_wallpaper() -> Result<Wallpaper, DesktopError> {
    let _com = ComApartment::enter()?;
    // SAFETY: COM is initialised on this thread for the lifetime of `_com`,
    // and the interface is released before `_com` is dropped because it is
    // declared later.
    let desktop: IDesktopWallpaper =
        unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)? };

    let reported = match primary_monitor(&desktop) {
        // SAFETY: the monitor id is a valid string owned by `monitor`.
        Some(monitor) => unsafe { desktop.GetWallpaper(monitor.as_pcwstr()) },
        // No usable monitor list: ask for the wallpaper shared by all of them.
        // SAFETY: a null monitor id is documented to mean "all monitors".
        None => unsafe { desktop.GetWallpaper(PCWSTR::null()) },
    }
    .map(CoString)?
    .to_string()?;

    // SAFETY: plain getter on a live interface.
    let background = from_colorref(unsafe { desktop.GetBackgroundColor()? }.0);
    let transcoded = transcoded_wallpaper();

    windows_wallpaper(&reported, transcoded.as_deref(), background)
        .ok_or(DesktopError::MissingFile(reported))
}

/// The copy of the current wallpaper that Windows rewrites on every
/// wallpaper change, including slideshow advances.
pub fn transcoded_wallpaper() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join(TRANSCODED_WALLPAPER))
}

/// With several monitors the shell is themed from the primary one, which is
/// the monitor whose top-left corner is the desktop origin. Monitors that
/// are listed but detached have no rectangle and are skipped.
fn primary_monitor(desktop: &IDesktopWallpaper) -> Option<CoString> {
    // SAFETY: plain getter on a live interface.
    let count = unsafe { desktop.GetMonitorDevicePathCount() }.ok()?;
    let mut first_attached = None;
    for index in 0..count {
        // SAFETY: `index` is below the count just returned.
        let Ok(monitor) = (unsafe { desktop.GetMonitorDevicePathAt(index) }).map(CoString) else {
            continue;
        };
        // SAFETY: the monitor id is a valid string owned by `monitor`.
        let Ok(rect) = (unsafe { desktop.GetMonitorRECT(monitor.as_pcwstr()) }) else {
            continue;
        };
        if rect.left == 0 && rect.top == 0 {
            return Some(monitor);
        }
        first_attached.get_or_insert(monitor);
    }
    first_attached
}

/// Keeps COM initialised on the current thread.
struct ComApartment {
    /// False when the thread was already initialised in another mode by
    /// someone else; COM is usable but the matching uninitialise is theirs.
    owned: bool,
}

impl ComApartment {
    fn enter() -> Result<Self, DesktopError> {
        // SAFETY: no reserved pointer is passed; a successful call is paired
        // with `CoUninitialize` in `Drop`.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { owned: false });
        }
        result.ok().map_err(DesktopError::ComInit)?;
        Ok(Self { owned: true })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: pairs with the successful `CoInitializeEx` in `enter`.
            unsafe { CoUninitialize() };
        }
    }
}

/// A string allocated by COM, freed when dropped.
struct CoString(PWSTR);

impl CoString {
    fn as_pcwstr(&self) -> PCWSTR {
        PCWSTR(self.0.0)
    }

    fn to_string(&self) -> Result<String, DesktopError> {
        if self.0.is_null() {
            return Ok(String::new());
        }
        // SAFETY: non-null strings returned by COM are null-terminated.
        unsafe { self.0.to_string() }.map_err(|_| DesktopError::InvalidPath)
    }
}

impl Drop for CoString {
    fn drop(&mut self) {
        // SAFETY: the pointer came from the COM allocator and is freed once;
        // freeing null is allowed.
        unsafe { CoTaskMemFree(Some(self.0.0.cast::<c_void>())) };
    }
}
