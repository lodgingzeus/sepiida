//! The Direct2D effects the blur brush chains, described to the compositor
//! through `IGraphicsEffectD2D1Interop`: a Gaussian blur of the backdrop,
//! a flood of the tint colour, and a source-over composite of the two.
//! Ported from TranslucentTB's `GaussianBlurEffect`, `FloodEffect` and
//! `CompositeEffect` (GPL-3.0), as carried in Windhawk's styler mods.

use std::cell::RefCell;

use windows::{
    Foundation::{IPropertyValue, PropertyValue},
    Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
    },
    UI::Color,
    Win32::{
        Foundation::{E_BOUNDS, E_INVALIDARG},
        System::WinRT::Graphics::Direct2D::{
            GRAPHICS_EFFECT_PROPERTY_MAPPING, IGraphicsEffectD2D1Interop,
            IGraphicsEffectD2D1Interop_Impl,
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR, Result, implement},
};

/// `CLSID_D2D1GaussianBlur` (d2d1effects.h).
const GAUSSIAN_BLUR: GUID = GUID::from_u128(0x1feb6d69_2fe6_4ac9_8c58_1d7f93e7a6a5);
/// `CLSID_D2D1Flood`.
const FLOOD: GUID = GUID::from_u128(0x61c23c20_ae69_4d8e_94cf_50078df638f2);
/// `CLSID_D2D1Composite`.
const COMPOSITE: GUID = GUID::from_u128(0x48fc9f51_f6ac_48f1_8b58_3b28ac46f76d);

/// `D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED`.
const OPTIMIZATION_BALANCED: u32 = 1;
/// `D2D1_BORDER_MODE_SOFT`.
const BORDER_MODE_SOFT: u32 = 0;
/// `D2D1_COMPOSITE_MODE_SOURCE_OVER`.
const SOURCE_OVER: u32 = 0;

fn value(inspectable: Result<windows::core::IInspectable>) -> Result<IPropertyValue> {
    inspectable?.cast()
}

/// A blur of `source` (`BlurAmount` is the standard deviation, in pixels).
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
pub struct GaussianBlur {
    pub source: IGraphicsEffectSource,
    pub amount: f32,
    name: RefCell<HSTRING>,
}

impl GaussianBlur {
    pub fn new(source: IGraphicsEffectSource, amount: f32) -> Self {
        Self {
            source,
            amount,
            name: RefCell::new(HSTRING::from("BlurEffect")),
        }
    }
}

impl IGraphicsEffectSource_Impl for GaussianBlur_Impl {}

impl IGraphicsEffect_Impl for GaussianBlur_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for GaussianBlur_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(GAUSSIAN_BLUR)
    }
    fn GetNamedPropertyMapping(
        &self,
        _name: &PCWSTR,
        _index: *mut u32,
        _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> Result<()> {
        // Nothing is animated by name.
        Err(E_INVALIDARG.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(3)
    }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        match index {
            0 => value(PropertyValue::CreateSingle(self.amount)),
            1 => value(PropertyValue::CreateUInt32(OPTIMIZATION_BALANCED)),
            2 => value(PropertyValue::CreateUInt32(BORDER_MODE_SOFT)),
            _ => Err(E_BOUNDS.into()),
        }
    }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        match index {
            0 => Ok(self.source.clone()),
            _ => Err(E_BOUNDS.into()),
        }
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(1)
    }
}

/// A plane of one colour (straight alpha).
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
pub struct Flood {
    pub color: Color,
    name: RefCell<HSTRING>,
}

impl Flood {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            name: RefCell::new(HSTRING::from("FloodEffect")),
        }
    }
}

impl IGraphicsEffectSource_Impl for Flood_Impl {}

impl IGraphicsEffect_Impl for Flood_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for Flood_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(FLOOD)
    }
    fn GetNamedPropertyMapping(
        &self,
        _name: &PCWSTR,
        _index: *mut u32,
        _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> Result<()> {
        Err(E_INVALIDARG.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(1)
    }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        let c = self.color;
        let channel = |v: u8| f32::from(v) / 255.0;
        match index {
            0 => value(PropertyValue::CreateSingleArray(&[
                channel(c.R),
                channel(c.G),
                channel(c.B),
                channel(c.A),
            ])),
            _ => Err(E_BOUNDS.into()),
        }
    }
    fn GetSource(&self, _index: u32) -> Result<IGraphicsEffectSource> {
        Err(E_BOUNDS.into())
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(0)
    }
}

/// `sources` drawn in order, each over the ones before.
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
pub struct Composite {
    pub sources: Vec<IGraphicsEffectSource>,
    name: RefCell<HSTRING>,
}

impl Composite {
    pub fn new(sources: Vec<IGraphicsEffectSource>) -> Self {
        Self {
            sources,
            name: RefCell::new(HSTRING::from("CompositeEffect")),
        }
    }
}

impl IGraphicsEffectSource_Impl for Composite_Impl {}

impl IGraphicsEffect_Impl for Composite_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for Composite_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(COMPOSITE)
    }
    fn GetNamedPropertyMapping(
        &self,
        _name: &PCWSTR,
        _index: *mut u32,
        _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> Result<()> {
        Err(E_INVALIDARG.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(1)
    }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        match index {
            0 => value(PropertyValue::CreateUInt32(SOURCE_OVER)),
            _ => Err(E_BOUNDS.into()),
        }
    }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.sources.get(i))
            .cloned()
            .ok_or_else(|| E_BOUNDS.into())
    }
    fn GetSourceCount(&self) -> Result<u32> {
        u32::try_from(self.sources.len()).map_err(|_| E_BOUNDS.into())
    }
}
