//! A selector's `[Property=Value]`, checked as the Windhawk stylers check
//! it: the value is parsed to the property's type through a one-setter
//! `Style` (`attached::resolve`) and compared with the element's local
//! value. No local value means no match; a bound one (read as null or as a
//! binding expression) is compared by its base value, as Windhawk does.
//! Strings compare as text, numbers as numbers; other types never match.

use sepiida_core::shell::{markup::condition_document, rules::Condition};
use windows::{
    Foundation::{IPropertyValue, PropertyType},
    core::{IInspectable, IUnknown, Interface},
};

use super::attached::{element, is_unset, read, resolve};
use crate::{properties::Xaml, xamlom::InstanceHandle};

/// Whether the element behind `handle`, of `type_name`, holds every
/// condition: `Ok(None)` if so, `Ok(Some(what was read))` for the first
/// that fails, `Err` with the step that could not be done.
pub fn check(
    xaml: &Xaml,
    handle: InstanceHandle,
    type_name: &str,
    conditions: &[Condition],
) -> Result<Option<String>, String> {
    let element = element(xaml, handle)?;
    for condition in conditions {
        let document = condition_document(type_name, condition.property, condition.value)
            .ok_or_else(|| format!("[{}] cannot be checked", condition.property))?;
        let (property, expected) = resolve(&document).map_err(str::to_owned)?;
        // SAFETY: a live element and property; `out` receives an owned
        // reference.
        let local = read(|out| unsafe { element.ReadLocalValue(property.as_raw(), out) })
            .ok_or("ReadLocalValue failed")?;
        let value = match local {
            Some(v) if is_unset(&v) => {
                return Ok(Some(format!("{} has no local value", condition.property)));
            }
            Some(v) if v.cast::<IPropertyValue>().is_ok() => Some(v),
            // SAFETY: as above.
            _ => read(|out| unsafe { element.GetAnimationBaseValue(property.as_raw(), out) })
                .flatten(),
        };
        match value {
            Some(v) if same(&v, &expected) => {}
            Some(v) => return Ok(Some(format!("{} is {}", condition.property, describe(&v)))),
            None => return Ok(Some(format!("{} is null", condition.property))),
        }
    }
    Ok(None)
}

/// A value for the log: text (non-ASCII as `\u{…}`), a number, or the
/// class name.
fn describe(value: &IUnknown) -> String {
    if let Ok(boxed) = value.cast::<IPropertyValue>() {
        if let Ok(text) = boxed.GetString() {
            let shown: String = text
                .to_string_lossy()
                .chars()
                .take(40)
                .map(|c| match c {
                    ' '..='~' => c.to_string(),
                    _ => format!("\\u{{{:X}}}", u32::from(c)),
                })
                .collect();
            return format!("\"{shown}\"");
        }
        if let Ok(number) = boxed.GetDouble() {
            return number.to_string();
        }
    }
    value
        .cast::<IInspectable>()
        .and_then(|i| i.GetRuntimeClassName())
        .map_or_else(|_| "of an unknown type".to_owned(), |c| format!("a {c}"))
}

/// Whether two boxed values are equal as text or as numbers.
fn same(a: &IUnknown, b: &IUnknown) -> bool {
    let (Ok(a), Ok(b)) = (a.cast::<IPropertyValue>(), b.cast::<IPropertyValue>()) else {
        return false;
    };
    let (Ok(kind_a), Ok(kind_b)) = (a.Type(), b.Type()) else {
        return false;
    };
    if kind_a == PropertyType::String || kind_b == PropertyType::String {
        return kind_a == kind_b
            && matches!((a.GetString(), b.GetString()), (Ok(x), Ok(y)) if x == y);
    }
    matches!((a.GetDouble(), b.GetDouble()), (Ok(x), Ok(y)) if x == y)
}
