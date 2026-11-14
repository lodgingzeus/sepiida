//! `Paint::Tint`: Windows' own `AcrylicBrush` stays on the element and gets
//! our tint through `IAcrylicBrush` (exact values in, exact values out, no
//! strings to parse).
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
    xamlom::{IAcrylicBrush, InstanceHandle},
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
    brush: IAcrylicBrush,
    state: BrushTint,
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
        tinted.state.applied(target);
        let now = read_tint(&tinted.brush);
        log!(
            "brush {value:#x} tinted {} at {}: {}",
            target.color,
            target.opacity,
            match now {
                Some(now) if now.color == target.color => "kept",
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
        let brush: IAcrylicBrush = match object.cast() {
            Ok(brush) => brush,
            Err(error) => {
                log!("value {value:#x} is not an AcrylicBrush ({error}); left as it is");
                return None;
            }
        };
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
fn log_details(xaml: &Xaml, value: InstanceHandle, brush: &IAcrylicBrush, current: Tint) {
    let (mut source, mut fallback_only) = (-1i32, 0u8);
    // SAFETY: plain calls on a live interface with writable out values.
    let (source_hr, fallback_hr) = unsafe {
        (
            brush.get_BackgroundSource(&mut source),
            brush.get_AlwaysUseFallback(&mut fallback_only),
        )
    };
    log!(
        "AcrylicBrush {value:#x}: tint {} at {}, background source {} ({source_hr}), \
         always fallback {} ({fallback_hr})",
        current.color,
        current.opacity,
        match source {
            0 => "HostBackdrop",
            1 => "Backdrop",
            _ => "unknown",
        },
        fallback_only != 0,
    );
    xaml.log_chain(value, &LOGGED_PROPERTIES);
}

fn read_tint(brush: &IAcrylicBrush) -> Option<Tint> {
    let mut color = windows::UI::Color::default();
    let mut opacity = 0.0;
    // SAFETY: plain calls on a live interface with writable out values.
    let (color_hr, opacity_hr) = unsafe {
        (
            brush.get_TintColor(&mut color),
            brush.get_TintOpacity(&mut opacity),
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
        color: Argb::new(color.A, color.R, color.G, color.B),
        opacity,
    })
}

/// Sets colour, then opacity. A failed opacity after a set colour still
/// counts: the colour is what shows.
fn write_tint(brush: &IAcrylicBrush, tint: Tint) -> bool {
    let color = windows::UI::Color {
        A: tint.color.a,
        R: tint.color.r,
        G: tint.color.g,
        B: tint.color.b,
    };
    // SAFETY: plain calls on a live interface.
    let (color_hr, opacity_hr): (HRESULT, HRESULT) = unsafe {
        (
            brush.put_TintColor(color),
            brush.put_TintOpacity(tint.opacity),
        )
    };
    log_failure("IAcrylicBrush::put_TintOpacity", opacity_hr);
    log_failure("IAcrylicBrush::put_TintColor", color_hr)
}
