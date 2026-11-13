//! The COM objects XAML Diagnostics creates: a class factory, and the
//! `IObjectWithSite` object ("TAP") whose site is the diagnostics service.

use core::{cell::RefCell, ffi::c_void};

use windows::{
    Win32::{
        Foundation::{CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, E_UNEXPECTED},
        System::{
            Com::{IClassFactory, IClassFactory_Impl},
            Ole::{IObjectWithSite, IObjectWithSite_Impl},
        },
    },
    core::{BOOL, GUID, IUnknown, Interface, Ref, Result, implement},
};

use crate::{guard, log::log, session};

#[implement(IClassFactory)]
pub struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Result<()> {
        guard(
            "IClassFactory::CreateInstance",
            Err(E_UNEXPECTED.into()),
            || {
                if iid.is_null() || out.is_null() {
                    return Err(E_POINTER.into());
                }
                // SAFETY: checked non-null above.
                unsafe { *out = core::ptr::null_mut() };
                if !outer.is_null() {
                    return Err(CLASS_E_NOAGGREGATION.into());
                }
                let tap: IObjectWithSite = Tap::default().into();
                // SAFETY: `iid` and `out` are valid, checked above.
                unsafe { tap.query(iid, out) }.ok()
            },
        )
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[implement(IObjectWithSite)]
#[derive(Default)]
pub struct Tap {
    site: RefCell<Option<IUnknown>>,
}

impl IObjectWithSite_Impl for Tap_Impl {
    fn SetSite(&self, site: Ref<IUnknown>) -> Result<()> {
        guard("IObjectWithSite::SetSite", Err(E_UNEXPECTED.into()), || {
            let site = site.cloned();
            let result = match &site {
                Some(site) => session::attach(site),
                None => {
                    log!("site cleared");
                    session::detach();
                    Ok(())
                }
            };
            if let Err(error) = &result {
                log!("attach failed: {error}");
            }
            if let Ok(mut slot) = self.site.try_borrow_mut() {
                *slot = site;
            }
            result
        })
    }

    fn GetSite(&self, iid: *const GUID, out: *mut *mut c_void) -> Result<()> {
        guard("IObjectWithSite::GetSite", Err(E_UNEXPECTED.into()), || {
            if iid.is_null() || out.is_null() {
                return Err(E_POINTER.into());
            }
            // SAFETY: checked non-null above.
            unsafe { *out = core::ptr::null_mut() };
            let site = self.site.try_borrow().map_err(|_| E_FAIL)?;
            match site.as_ref() {
                // SAFETY: `iid` and `out` are valid, checked above.
                Some(site) => unsafe { site.query(iid, out) }.ok(),
                None => Err(E_FAIL.into()),
            }
        })
    }
}
