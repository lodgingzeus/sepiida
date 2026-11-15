//! XAML objects from theme lines (`Value::Xaml`), made with
//! `XamlReader.Load` on the XAML thread that styles the element. The
//! document comes from `sepiida_core::shell::markup::document`, which only
//! passes rules built into sepiida and refuses anything reaching outside
//! the process.
//!
//! WinUI 3's reader is activated through `RoGetActivationFactory` like
//! system XAML's; if Windows does not know the class in this process, the
//! factory comes from the `Microsoft.UI.Xaml.dll` File Explorer already has
//! loaded (never a DLL loaded by us).

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use sepiida_core::shell::framework::Framework;
use windows::{
    Win32::System::{
        LibraryLoader::{GetModuleHandleW, GetProcAddress},
        WinRT::RoGetActivationFactory,
    },
    core::{HRESULT, HSTRING, IUnknown, Interface, s, w},
};

use crate::{
    log::log,
    properties::log_failure,
    xamlom::{IWinUiXamlReaderStatics, IXamlReaderStatics},
};

const XAML_READER: &str = "Windows.UI.Xaml.Markup.XamlReader";
const WINUI_XAML_READER: &str = "Microsoft.UI.Xaml.Markup.XamlReader";

/// Set once the way WinUI 3's reader was found has been logged.
static WINUI_READER_LOGGED: AtomicBool = AtomicBool::new(false);

type DllGetActivationFactory = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;

/// The object `document` describes, in `framework`'s XAML, or `None`
/// (logged) if XAML refused it.
pub fn load(document: &str, framework: Framework) -> Option<IUnknown> {
    let text = HSTRING::from(document);
    let mut raw: *mut c_void = core::ptr::null_mut();
    // SAFETY (both arms): `text` outlives the call and is only borrowed (an
    // `HSTRING` is one handle pointer); `raw` receives an owned reference.
    let hr = match framework {
        Framework::System => {
            let reader = system_reader()?;
            unsafe { reader.Load(core::mem::transmute_copy(&text), &mut raw) }
        }
        Framework::WinUi => {
            let reader = winui_reader()?;
            unsafe { reader.Load(core::mem::transmute_copy(&text), &mut raw) }
        }
    };
    if !log_failure("XamlReader.Load", hr) || raw.is_null() {
        log!("XAML refused: {document}");
        return None;
    }
    // SAFETY: a non-null, owned interface pointer from XAML.
    Some(unsafe { IUnknown::from_raw(raw) })
}

fn system_reader() -> Option<IXamlReaderStatics> {
    let class = HSTRING::from(XAML_READER);
    // SAFETY: a plain WinRT call with a valid class name; COM is initialised
    // on the XAML thread this runs on.
    match unsafe { RoGetActivationFactory(&class) } {
        Ok(reader) => Some(reader),
        Err(error) => {
            log!("XamlReader is not available: {error}");
            None
        }
    }
}

fn winui_reader() -> Option<IWinUiXamlReaderStatics> {
    let class = HSTRING::from(WINUI_XAML_READER);
    // SAFETY: as above.
    let activated = unsafe { RoGetActivationFactory::<IWinUiXamlReaderStatics>(&class) };
    let (reader, how) = match activated {
        Ok(reader) => (reader, "RoGetActivationFactory".to_owned()),
        Err(error) => match factory_from_loaded_module(&class) {
            Some(reader) => (
                reader,
                format!("Microsoft.UI.Xaml.dll (RoGetActivationFactory: {error})"),
            ),
            None => {
                log!("WinUI XamlReader is not available: {error}");
                return None;
            }
        },
    };
    if !WINUI_READER_LOGGED.swap(true, Ordering::AcqRel) {
        log!("WinUI XamlReader from {how}");
    }
    Some(reader)
}

/// The class's factory from `Microsoft.UI.Xaml.dll`'s
/// `DllGetActivationFactory`, if the process has that module loaded.
fn factory_from_loaded_module(class: &HSTRING) -> Option<IWinUiXamlReaderStatics> {
    // SAFETY: a valid name; the handle stays valid while XAML keeps its
    // framework loaded, which it does for the life of the process.
    let module = unsafe { GetModuleHandleW(w!("Microsoft.UI.Xaml.dll")) }.ok()?;
    // SAFETY: a module handle and a static, NUL-terminated export name.
    let proc = unsafe { GetProcAddress(module, s!("DllGetActivationFactory")) }?;
    // SAFETY: the documented signature of a WinRT component's export.
    let get_factory = unsafe {
        core::mem::transmute::<unsafe extern "system" fn() -> isize, DllGetActivationFactory>(proc)
    };
    let mut raw: *mut c_void = core::ptr::null_mut();
    // SAFETY: `class` outlives the call and is only borrowed; `raw`
    // receives an owned reference.
    let hr = unsafe { get_factory(core::mem::transmute_copy(class), &mut raw) };
    if !log_failure("DllGetActivationFactory", hr) || raw.is_null() {
        return None;
    }
    // SAFETY: a non-null, owned interface pointer.
    let factory = unsafe { IUnknown::from_raw(raw) };
    match factory.cast() {
        Ok(reader) => Some(reader),
        Err(error) => {
            log!("the XamlReader factory is not IXamlReaderStatics: {error}");
            None
        }
    }
}
