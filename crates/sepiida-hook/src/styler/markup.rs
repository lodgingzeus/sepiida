//! XAML objects from theme lines (`Value::Xaml`), made with
//! `XamlReader.Load` on the XAML thread that styles the element. The
//! document comes from `sepiida_core::shell::markup::document`, which only
//! passes rules built into sepiida and refuses anything reaching outside
//! the process.

use core::ffi::c_void;

use windows::{
    Win32::System::WinRT::RoGetActivationFactory,
    core::{HSTRING, IUnknown, Interface},
};

use crate::{log::log, properties::log_failure, xamlom::IXamlReaderStatics};

const XAML_READER: &str = "Windows.UI.Xaml.Markup.XamlReader";

/// The object `document` describes, or `None` (logged) if XAML refused it.
pub fn load(document: &str) -> Option<IUnknown> {
    let class = HSTRING::from(XAML_READER);
    // SAFETY: a plain WinRT call with a valid class name; COM is initialised
    // on the XAML thread this runs on.
    let reader: IXamlReaderStatics = match unsafe { RoGetActivationFactory(&class) } {
        Ok(reader) => reader,
        Err(error) => {
            log!("XamlReader is not available: {error}");
            return None;
        }
    };
    let text = HSTRING::from(document);
    let mut raw: *mut c_void = core::ptr::null_mut();
    // SAFETY: `text` outlives the call and is only borrowed (an `HSTRING` is
    // one handle pointer); `raw` receives an owned reference.
    let hr = unsafe { reader.Load(core::mem::transmute_copy(&text), &mut raw) };
    if !log_failure("XamlReader.Load", hr) || raw.is_null() {
        log!("XAML refused: {document}");
        return None;
    }
    // SAFETY: a non-null, owned interface pointer from XAML.
    Some(unsafe { IUnknown::from_raw(raw) })
}
