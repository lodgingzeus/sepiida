//! `Paint::Tint`: Windows' own `AcrylicBrush` stays on the element and gets
//! our tint through `IAcrylicBrush`, and our fallback colour through
//! `IXamlCompositionBrushBase` (exact values in, exact values out, no
//! strings to parse). The fallback is what shows when acrylic cannot be
//! drawn, so without it a VM or battery saver shows no change at all.
//!
//! State is kept per brush, not per element: the brush belongs to Windows,
//! may be shared by several elements and outlives an element that leaves
//! the tree (a flyout closing). Each tinted brush is held strongly until
//! the restore, which puts its original tint back even when no element
//! uses it any more.

use core::ffi::c_void;

use sepiida_core::{
    Color,
    shell::tint::{Argb, BrushTint, Opacity, Tint},
};
use windows::core::{HRESULT, IUnknown, Interface};

use crate::{
    log::log,
    properties::{Xaml, log_failure},
    xamlom::{IAcrylicBrush, IXamlCompositionBrushBase, InstanceHandle},
};

/// Brush properties logged when a brush is first tinted; together they
/// decide how the acrylic looks.
const LOGGED_PROPERTIES: [&str; 5] = [
    "TintColor",
    "TintOpacity",
    "TintLuminosityOpacity",
    "FallbackColor",
    "BackgroundSource",
];

struct Tinted {
    /// COM identity (the canonical `IUnknown` pointer) of the brush.
    identity: *mut c_void,
    brush: Brush,
    state: BrushTint,
}

/// The two interfaces of one acrylic brush; the base is optional, since
/// the tint alone still works without it.
struct Brush {
    acrylic: IAcrylicBrush,
    base: Option<IXamlCompositionBrushBase>,
}

#[derive(Default)]
pub struct Tints {
    brushes: Vec<Tinted>,
}

impl Tints {
    /// Tints the brush whose handle is `value` (a target's current
    /// property value) with `color`, at `opacity` or Windows' own opacity.
    /// Returns false, changing nothing, if it is not an `AcrylicBrush` or
    /// refuses the tint.
    pub fn apply(
        &mut self,
        xaml: &Xaml,
        value: InstanceHandle,
        color: Color,
        opacity: Option<Opacity>,
    ) -> bool {
        let Some(slot) = self.slot(xaml, value) else {
            return false;
        };
        let tinted = &mut self.brushes[slot];
        let Some(current) = read_tint(&tinted.brush) else {
            return false;
        };
        if tinted.state.observe(current) {
            log!(
                "Windows changed the tint of brush {value:#x}; new original {} at {}",
                current.color,
                current.opacity
            );
        }
        let target = tinted.state.target(color, opacity);
        if !write_tint(&tinted.brush, target) {
            return false;
        }
        let now = read_tint(&tinted.brush);
        // What the brush really holds now counts as ours: if a part of
        // the write failed, the next push must not take that for a change
        // by Windows (and our colour for the original).
        tinted.state.applied(now.unwrap_or(target));
        log!(
            "brush {value:#x} tinted {} at {}, fallback {:?}: {}",
            target.color,
            target.opacity,
            target.fallback.map(|f| f.to_string()),
            match now {
                Some(now) if now.color == target.color && now.fallback == target.fallback => {
                    "kept"
                }
                Some(_) => "NOT kept",
                None => "could not read back",
            }
        );
        true
    }

    /// Puts the original tint back on every brush we tinted and lets go of
    /// them; the next `apply` reads the originals afresh.
    pub fn restore_all(&mut self) {
        for tinted in self.brushes.drain(..) {
            let original = tinted.state.original();
            let ok = write_tint(&tinted.brush, original);
            log!(
                "restore tint {} at {} on brush {:?}: {ok}",
                original.color,
                original.opacity,
                tinted.identity
            );
        }
    }

    /// The brush behind `value` among those already tinted, or newly
    /// recorded if it is an `AcrylicBrush`.
    fn slot(&mut self, xaml: &Xaml, value: InstanceHandle) -> Option<usize> {
        let object = xaml.object(value)?;
        let identity = match object.cast::<IUnknown>() {
            Ok(unknown) => unknown.as_raw(),
            Err(error) => {
                log!("brush {value:#x} has no IUnknown identity: {error}");
                return None;
            }
        };
        if let Some(slot) = self.brushes.iter().position(|t| t.identity == identity) {
            return Some(slot);
        }
        let acrylic: IAcrylicBrush = match object.cast() {
            Ok(brush) => brush,
            Err(error) => {
                log!("value {value:#x} is not an AcrylicBrush ({error}); left as it is");
                return None;
            }
        };
        let base = match object.cast::<IXamlCompositionBrushBase>() {
            Ok(base) => Some(base),
            Err(error) => {
                log!("brush {value:#x}: no fallback colour interface ({error})");
                None
            }
        };
        let brush = Brush { acrylic, base };
        let current = read_tint(&brush)?;
        log_details(xaml, value, &brush, current);
        self.brushes.push(Tinted {
            identity,
            brush,
            state: BrushTint::new(current),
        });
        Some(self.brushes.len() - 1)
    }
}

/// Logs what decides the look of a brush seen for the first time.
fn log_details(xaml: &Xaml, value: InstanceHandle, brush: &Brush, current: Tint) {
    let brush = &brush.acrylic;
    let (mut source, mut fallback_only) = (-1i32, 0u8);
    // SAFETY: plain calls on a live interface with writable out values.
    let (source_hr, fallback_hr) = unsafe {
        (
            brush.get_BackgroundSource(&mut source),
            brush.get_AlwaysUseFallback(&mut fallback_only),
        )
    };
    log!(
        "AcrylicBrush {value:#x}: tint {} at {}, fallback {:?}, background source {} \
         ({source_hr}), always fallback {} ({fallback_hr})",
        current.color,
        current.opacity,
        current.fallback.map(|f| f.to_string()),
        match source {
            0 => "HostBackdrop",
            1 => "Backdrop",
            _ => "unknown",
        },
        fallback_only != 0,
    );
    xaml.log_chain(value, &LOGGED_PROPERTIES);
}

fn read_tint(brush: &Brush) -> Option<Tint> {
    let mut color = windows::UI::Color::default();
    let mut opacity = 0.0;
    // SAFETY: plain calls on a live interface with writable out values.
    let (color_hr, opacity_hr) = unsafe {
        (
            brush.acrylic.get_TintColor(&mut color),
            brush.acrylic.get_TintOpacity(&mut opacity),
        )
    };
    if !log_failure("IAcrylicBrush::get_TintColor", color_hr)
        || !log_failure("IAcrylicBrush::get_TintOpacity", opacity_hr)
    {
        return None;
    }
    if !opacity.is_finite() {
        log!("TintOpacity {opacity} is not a number; brush left alone");
        return None;
    }
    Some(Tint {
        color: argb(color),
        opacity,
        fallback: brush.base.as_ref().and_then(read_fallback),
    })
}

fn read_fallback(base: &IXamlCompositionBrushBase) -> Option<Argb> {
    let mut color = windows::UI::Color::default();
    // SAFETY: plain call on a live interface with a writable out value.
    let hr = unsafe { base.get_FallbackColor(&mut color) };
    log_failure("IXamlCompositionBrushBase::get_FallbackColor", hr).then(|| argb(color))
}

fn argb(c: windows::UI::Color) -> Argb {
    Argb::new(c.A, c.R, c.G, c.B)
}

fn ui_color(c: Argb) -> windows::UI::Color {
    windows::UI::Color {
        A: c.a,
        R: c.r,
        G: c.g,
        B: c.b,
    }
}

/// Sets colour, opacity and fallback. Failures after a set colour still
/// count: the colour is what shows.
fn write_tint(brush: &Brush, tint: Tint) -> bool {
    // SAFETY: plain calls on live interfaces.
    let (color_hr, opacity_hr): (HRESULT, HRESULT) = unsafe {
        (
            brush.acrylic.put_TintColor(ui_color(tint.color)),
            brush.acrylic.put_TintOpacity(tint.opacity),
        )
    };
    log_failure("IAcrylicBrush::put_TintOpacity", opacity_hr);
    if let (Some(base), Some(fallback)) = (&brush.base, tint.fallback) {
        // SAFETY: plain call on a live interface.
        let hr = unsafe { base.put_FallbackColor(ui_color(fallback)) };
        log_failure("IXamlCompositionBrushBase::put_FallbackColor", hr);
    }
    log_failure("IAcrylicBrush::put_TintColor", color_hr)
}
