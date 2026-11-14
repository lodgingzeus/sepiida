//! The blur brush: a XAML brush derived from `XamlCompositionBrushBase`,
//! ported from TranslucentTB's `XamlBlurBrush` (GPL-3.0) as carried in
//! Windhawk's styler mods. When XAML connects it to an element, it hands
//! XAML a composition brush: the backdrop blurred, with the tint flooded
//! over it (`effects`).
//!
//! Deriving from a XAML class means COM aggregation, which the `windows`
//! crate cannot express, so the object is built by hand: our struct is the
//! controlling ("outer") object. It answers `IUnknown`/`IInspectable`,
//! `IXamlCompositionBrushBaseOverrides` and `ISolidColorBrush` itself
//! (Windows' toast code casts a toast's background to `ISolidColorBrush`
//! and crashes without it) and passes every other interface to XAML's
//! inner object. Every entry point catches panics: nothing may unwind
//! into the shell process.

use core::ffi::c_void;
use std::{
    cell::{OnceCell, RefCell},
    mem::offset_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU32, Ordering},
};

use windows::{
    Graphics::Effects::{IGraphicsEffect, IGraphicsEffectSource},
    UI::{
        Color,
        Composition::{CompositionBrush, CompositionEffectSourceParameter, Compositor},
    },
    Win32::{
        Foundation::{E_NOINTERFACE, E_POINTER, E_UNEXPECTED, S_OK},
        System::WinRT::RoGetActivationFactory,
    },
    core::{GUID, HRESULT, HSTRING, IInspectable, IUnknown, Interface, Result},
};

use super::effects::{Composite, Flood, GaussianBlur};
use crate::{
    log::log,
    xamlom::{
        IID_XAML_COMPOSITION_BRUSH_BASE_OVERRIDES, ISolidColorBrush,
        IXamlCompositionBrushBaseFactory, IXamlCompositionBrushBaseProtected,
    },
};

const BASE_CLASS: &str = "Windows.UI.Xaml.Media.XamlCompositionBrushBase";
const BACKDROP: &str = "backdrop";

type QueryInterface =
    unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT;
type RefCount = unsafe extern "system" fn(*mut c_void) -> u32;
type GetIids = unsafe extern "system" fn(*mut c_void, *mut u32, *mut *mut GUID) -> HRESULT;
type GetRuntimeClassName = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
type GetTrustLevel = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;

#[repr(C)]
struct InspectableVtbl {
    query_interface: QueryInterface,
    add_ref: RefCount,
    release: RefCount,
    get_iids: GetIids,
    get_runtime_class_name: GetRuntimeClassName,
    get_trust_level: GetTrustLevel,
}

#[repr(C)]
struct OverridesVtbl {
    base: InspectableVtbl,
    on_connected: unsafe extern "system" fn(*mut c_void) -> HRESULT,
    on_disconnected: unsafe extern "system" fn(*mut c_void) -> HRESULT,
}

#[repr(C)]
struct SolidVtbl {
    base: InspectableVtbl,
    get_color: unsafe extern "system" fn(*mut c_void, *mut Color) -> HRESULT,
    put_color: unsafe extern "system" fn(*mut c_void, Color) -> HRESULT,
}

/// What the brush draws.
struct Look {
    compositor: Compositor,
    amount: f32,
    tint: Color,
    /// Set while XAML has it connected to an element.
    connected: bool,
}

/// The controlling object. Each vtable pointer is one interface; its
/// address is what callers hold, and the struct is found from it by offset.
#[repr(C)]
struct Brush {
    identity: *const InspectableVtbl,
    overrides: *const OverridesVtbl,
    solid: *const SolidVtbl,
    refs: AtomicU32,
    /// XAML's inner, non-delegating object; set right after composition.
    inner: OnceCell<IInspectable>,
    look: RefCell<Look>,
}

/// Our reference to a blur brush, for colour changes.
pub struct BlurBrush {
    object: IUnknown,
    brush: *const Brush,
}

impl BlurBrush {
    /// A new blur brush, or `None` (logged) if XAML would not compose one.
    pub fn new(compositor: Compositor, amount: f32, tint: Color) -> Option<Self> {
        // SAFETY: a plain WinRT call with a valid class name, on the XAML
        // thread.
        let factory: IXamlCompositionBrushBaseFactory =
            match unsafe { RoGetActivationFactory(&HSTRING::from(BASE_CLASS)) } {
                Ok(factory) => factory,
                Err(error) => {
                    log!("no XamlCompositionBrushBase factory: {error}");
                    return None;
                }
            };
        let raw = Box::into_raw(Box::new(Brush {
            identity: &IDENTITY,
            overrides: &OVERRIDES,
            solid: &SOLID,
            refs: AtomicU32::new(1),
            inner: OnceCell::new(),
            look: RefCell::new(Look {
                compositor,
                amount,
                tint,
                connected: false,
            }),
        }));
        let (mut inner, mut instance) = (core::ptr::null_mut(), core::ptr::null_mut());
        // SAFETY: `raw` is our live object, whose first field is its
        // identity vtable; both out pointers are writable.
        let hr = unsafe { factory.CreateInstance(raw.cast(), &mut inner, &mut instance) };
        if hr.is_err() || inner.is_null() {
            log!("XamlCompositionBrushBase.CreateInstance: {hr}");
            // SAFETY: nobody else holds `raw` if composition failed; a
            // returned instance would hold a reference, released first.
            unsafe {
                if !instance.is_null() {
                    drop(IUnknown::from_raw(instance));
                }
                if !inner.is_null() {
                    drop(IInspectable::from_raw(inner));
                }
                release(raw);
            }
            return None;
        }
        // SAFETY: `inner` is an owned reference to XAML's inner object;
        // `instance` an owned reference through our outer object, which
        // our own initial reference outlives.
        unsafe {
            let _ = (*raw).inner.set(IInspectable::from_raw(inner));
            if !instance.is_null() {
                drop(IUnknown::from_raw(instance));
            }
        }
        Some(Self {
            // SAFETY: takes over our initial reference.
            object: unsafe { IUnknown::from_raw(raw.cast()) },
            brush: raw,
        })
    }

    /// The brush as XAML sees it.
    pub fn object(&self) -> &IUnknown {
        &self.object
    }

    /// Gives the brush a new tint; redrawn at once if connected.
    pub fn set_tint(&self, tint: Color) {
        // SAFETY: `object` keeps the struct alive; XAML calls it on this
        // same thread only.
        let brush = unsafe { &*self.brush };
        let connected = {
            let mut look = brush.look.borrow_mut();
            look.tint = tint;
            look.connected
        };
        if connected && let Err(error) = brush.connect() {
            log!("blur brush: new tint not drawn: {error}");
        }
    }
}

impl Brush {
    fn inner(&self) -> Result<&IInspectable> {
        self.inner.get().ok_or_else(|| E_UNEXPECTED.into())
    }

    /// Builds the composition brush and hands it to XAML.
    fn connect(&self) -> Result<()> {
        let brush = {
            let look = self.look.borrow();
            effect_brush(&look.compositor, look.amount, look.tint)?
        };
        let protected: IXamlCompositionBrushBaseProtected = self.inner()?.cast()?;
        // SAFETY: a live composition brush, borrowed for the call.
        unsafe { protected.put_CompositionBrush(brush.as_raw()) }.ok()?;
        self.look.borrow_mut().connected = true;
        Ok(())
    }

    /// The tint alone, without blur: when the effect cannot be made (weak
    /// graphics), so the surface is never left see-through.
    fn connect_plain(&self) -> Result<()> {
        let brush: CompositionBrush = {
            let look = self.look.borrow();
            let opaque = Color {
                A: 0xff,
                ..look.tint
            };
            look.compositor.CreateColorBrushWithColor(opaque)?.cast()?
        };
        let protected: IXamlCompositionBrushBaseProtected = self.inner()?.cast()?;
        // SAFETY: a live composition brush, borrowed for the call.
        unsafe { protected.put_CompositionBrush(brush.as_raw()) }.ok()?;
        self.look.borrow_mut().connected = true;
        Ok(())
    }

    fn disconnect(&self) -> Result<()> {
        self.look.borrow_mut().connected = false;
        let protected: IXamlCompositionBrushBaseProtected = self.inner()?.cast()?;
        // SAFETY: null clears the composition brush.
        unsafe { protected.put_CompositionBrush(core::ptr::null_mut()) }.ok()
    }
}

/// Backdrop → blur → tint flooded over it.
fn effect_brush(compositor: &Compositor, amount: f32, tint: Color) -> Result<CompositionBrush> {
    let backdrop: IGraphicsEffectSource =
        CompositionEffectSourceParameter::Create(&HSTRING::from(BACKDROP))?.cast()?;
    let blur: IGraphicsEffectSource = GaussianBlur::new(backdrop, amount).into();
    let flood: IGraphicsEffectSource = Flood::new(tint).into();
    let composite: IGraphicsEffect = Composite::new(vec![blur, flood]).into();
    let factory = compositor.CreateEffectFactory(&composite)?;
    let brush = factory.CreateBrush()?;
    brush.SetSourceParameter(&HSTRING::from(BACKDROP), &compositor.CreateBackdropBrush()?)?;
    brush.cast()
}

/// Runs an entry point's body, turning a panic into `E_UNEXPECTED`.
fn guarded(body: impl FnOnce() -> HRESULT) -> HRESULT {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(E_UNEXPECTED)
}

/// The struct an interface pointer belongs to.
///
/// # Safety
/// `this` must be the address of the field at `offset` in a live `Brush`.
unsafe fn brush<'a>(this: *mut c_void, offset: usize) -> &'a Brush {
    // SAFETY: per the contract, stepping back `offset` bytes is the start
    // of the struct.
    unsafe { &*this.cast::<u8>().sub(offset).cast::<Brush>() }
}

/// # Safety
/// COM's contract for `QueryInterface` on `this`.
unsafe fn query(this: &Brush, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    // SAFETY: checked non-null; COM passes a readable IID.
    let iid = unsafe { *iid };
    let ours: Option<*const c_void> = if iid == IUnknown::IID || iid == IInspectable::IID {
        Some((&raw const this.identity).cast())
    } else if iid == IID_XAML_COMPOSITION_BRUSH_BASE_OVERRIDES {
        Some((&raw const this.overrides).cast())
    } else if iid == ISolidColorBrush::IID {
        Some((&raw const this.solid).cast())
    } else {
        None
    };
    if let Some(pointer) = ours {
        this.refs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `out` is writable; the reference was just added.
        unsafe { *out = pointer.cast_mut() };
        return S_OK;
    }
    match this.inner.get() {
        // SAFETY: the inner object's own QueryInterface, with the caller's
        // pointers.
        Some(inner) => unsafe { inner.query(&iid, out) },
        None => {
            // SAFETY: `out` is writable.
            unsafe { *out = core::ptr::null_mut() };
            E_NOINTERFACE
        }
    }
}

/// # Safety
/// `this` must point at a live `Brush`.
unsafe fn add_ref(this: *const Brush) -> u32 {
    // SAFETY: per the contract.
    unsafe { (*this).refs.fetch_add(1, Ordering::Relaxed) + 1 }
}

/// # Safety
/// `this` must point at a live `Brush` the caller holds a reference to.
unsafe fn release(this: *const Brush) -> u32 {
    // SAFETY: per the contract.
    let left = unsafe { (*this).refs.fetch_sub(1, Ordering::AcqRel) }.wrapping_sub(1);
    if left == 0 {
        // A stray AddRef/Release while the inner object is torn down must
        // not free us twice.
        // SAFETY: the last reference is gone; the box is ours to free.
        unsafe {
            (*this).refs.store(1, Ordering::Relaxed);
            drop(Box::from_raw(this.cast_mut()));
        }
    }
    left
}

/// The `IUnknown`/`IInspectable` slots of one interface, found at `OFFSET`.
macro_rules! inspectable_slots {
    ($offset:expr) => {{
        unsafe extern "system" fn qi(
            this: *mut c_void,
            iid: *const GUID,
            out: *mut *mut c_void,
        ) -> HRESULT {
            // SAFETY: COM calls through the vtable at this offset.
            guarded(|| unsafe { query(brush(this, $offset), iid, out) })
        }
        unsafe extern "system" fn up(this: *mut c_void) -> u32 {
            // SAFETY: as above.
            unsafe { add_ref(brush(this, $offset)) }
        }
        unsafe extern "system" fn down(this: *mut c_void) -> u32 {
            // SAFETY: as above; the caller gives up its reference.
            unsafe { release(brush(this, $offset)) }
        }
        unsafe extern "system" fn iids(
            _: *mut c_void,
            count: *mut u32,
            iids: *mut *mut GUID,
        ) -> HRESULT {
            if count.is_null() || iids.is_null() {
                return E_POINTER;
            }
            // SAFETY: checked non-null.
            unsafe {
                *count = 0;
                *iids = core::ptr::null_mut();
            }
            S_OK
        }
        unsafe extern "system" fn class_name(this: *mut c_void, name: *mut *mut c_void) -> HRESULT {
            guarded(|| {
                // SAFETY: as above.
                let this = unsafe { brush(this, $offset) };
                match this.inner.get() {
                    // SAFETY: the inner object's own method, same contract.
                    Some(inner) => unsafe {
                        (Interface::vtable(inner).GetRuntimeClassName)(inner.as_raw(), name.cast())
                    },
                    None => E_UNEXPECTED,
                }
            })
        }
        unsafe extern "system" fn trust(_: *mut c_void, level: *mut i32) -> HRESULT {
            if level.is_null() {
                return E_POINTER;
            }
            // SAFETY: checked non-null; 0 is BaseTrust.
            unsafe { *level = 0 };
            S_OK
        }
        InspectableVtbl {
            query_interface: qi,
            add_ref: up,
            release: down,
            get_iids: iids,
            get_runtime_class_name: class_name,
            get_trust_level: trust,
        }
    }};
}

static IDENTITY: InspectableVtbl = inspectable_slots!(offset_of!(Brush, identity));

static OVERRIDES: OverridesVtbl = OverridesVtbl {
    base: inspectable_slots!(offset_of!(Brush, overrides)),
    on_connected,
    on_disconnected,
};

static SOLID: SolidVtbl = SolidVtbl {
    base: inspectable_slots!(offset_of!(Brush, solid)),
    get_color,
    put_color,
};

unsafe extern "system" fn on_connected(this: *mut c_void) -> HRESULT {
    guarded(|| {
        // SAFETY: XAML calls through our overrides vtable.
        let brush = unsafe { brush(this, offset_of!(Brush, overrides)) };
        if brush.look.borrow().connected {
            return S_OK;
        }
        match brush.connect() {
            Ok(()) => S_OK,
            Err(error) => {
                log!("blur brush not drawn: {error}; plain tint instead");
                if let Err(error) = brush.connect_plain() {
                    log!("plain tint not drawn either: {error}");
                }
                S_OK
            }
        }
    })
}

unsafe extern "system" fn on_disconnected(this: *mut c_void) -> HRESULT {
    guarded(|| {
        // SAFETY: as above.
        let brush = unsafe { brush(this, offset_of!(Brush, overrides)) };
        if let Err(error) = brush.disconnect() {
            log!("blur brush disconnect: {error}");
        }
        S_OK
    })
}

unsafe extern "system" fn get_color(this: *mut c_void, color: *mut Color) -> HRESULT {
    if color.is_null() {
        return E_POINTER;
    }
    guarded(|| {
        // SAFETY: callers come through our solid-colour vtable.
        let brush = unsafe { brush(this, offset_of!(Brush, solid)) };
        // SAFETY: checked non-null.
        unsafe { *color = brush.look.borrow().tint };
        S_OK
    })
}

unsafe extern "system" fn put_color(_: *mut c_void, _: Color) -> HRESULT {
    // Windows may set it; the tint stays ours.
    S_OK
}
