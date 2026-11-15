//! Reads the size values (`markup::SIZE_PROPERTIES`) an element holds
//! locally, to notice when Windows' own code sets one of ours again. The
//! `DependencyProperty` of each is resolved once per thread through a
//! one-setter `Style` (`attached::resolve`), as for conditions.

use std::collections::HashMap;

use sepiida_core::shell::markup::size_document;
use windows::{
    Foundation::IPropertyValue,
    core::{IUnknown, Interface},
};

use crate::{
    log::log,
    properties::Xaml,
    styler::attached::{element, is_unset, read, resolve},
    xamlom::InstanceHandle,
};

#[derive(Default)]
pub(super) struct Sizes {
    /// Each property's `DependencyProperty`; `None` if it did not resolve
    /// (logged once).
    properties: HashMap<&'static str, Option<IUnknown>>,
}

impl Sizes {
    /// The element's local value of `property` as a number: `Ok(None)`
    /// for no local value or one that is not a number.
    pub(super) fn local(
        &mut self,
        xaml: &Xaml,
        handle: InstanceHandle,
        property: &'static str,
    ) -> Result<Option<f64>, String> {
        let resolved = self
            .properties
            .entry(property)
            .or_insert_with(|| {
                let resolved = size_document(property)
                    .ok_or("not a size property")
                    .and_then(|document| resolve(&document));
                match resolved {
                    Ok((property, _)) => Some(property),
                    Err(why) => {
                        log!("{property}: no DependencyProperty: {why}");
                        None
                    }
                }
            })
            .clone()
            .ok_or("no DependencyProperty")?;
        let element = element(xaml, handle)?;
        // SAFETY: a live element and property; `out` receives an owned
        // reference.
        let local = read(|out| unsafe { element.ReadLocalValue(resolved.as_raw(), out) })
            .ok_or("ReadLocalValue failed")?;
        Ok(local
            .filter(|v| !is_unset(v))
            .and_then(|v| v.cast::<IPropertyValue>().ok())
            .and_then(|v| v.GetDouble().ok()))
    }
}
