//! Fills over the glass: content-background fills become transparent
//! black, fills that must stay solid (edit controls, where the caret
//! inverts only colour bits) are made opaque, and the navigation pane
//! divider is drawn translucent.

use std::sync::OnceLock;

use sepiida_core::shell::gdi_glass::{self, DIVIDER_ALPHA, Rect};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, RECT},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFOHEADER, BLACK_BRUSH, BLENDFUNCTION, BS_SOLID,
        COLOR_WINDOW, CreateDIBPatternBrushPt, CreateSolidBrush, DC_BRUSH, DIB_RGB_COLORS,
        DeleteObject, FillRect, GetCurrentObject, GetDCBrushColor, GetObjectW, GetStockObject,
        HBRUSH, HDC, HGDIOBJ, LOGBRUSH, LOGPEN, OBJ_BRUSH, OBJ_PEN, PATCOPY, PS_SOLID, PatBlt,
        Polyline, ROP_CODE, SelectObject,
    },
    UI::{
        Controls::{
            BP_PAINTPARAMS, BPBF_TOPDOWNDIB, BeginBufferedPaint, BufferedPaintSetAlpha,
            EndBufferedPaint,
        },
        WindowsAndMessaging::GetClassNameW,
    },
};
use windows_core::BOOL;

use super::{
    install::{Seen, hook_guard, seen},
    state,
};

/// `PatBlt` raster operation: destination OR pattern.
const DEST_OR_PATTERN: ROP_CODE = ROP_CODE(0x00fa_0089);

pub(super) fn to_rect(r: &RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

pub(super) fn from_rect(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// Fills that are made opaque: edit controls (the system caret inverts
/// only the colour bits). The mod adds DirectUI element borders, which
/// import tables cannot reach.
pub(super) fn is_opaque_fill(window: HWND) -> bool {
    let mut class = [0u16; 16];
    // SAFETY: a window handle; the buffer is ours.
    let length = unsafe { GetClassNameW(window, &mut class) };
    let length = usize::try_from(length).unwrap_or(0).min(class.len());
    length == 4
        && class[..length]
            .iter()
            .zip("edit".encode_utf16())
            .all(|(&c, e)| u8::try_from(c).is_ok_and(|c| u16::from(c.to_ascii_lowercase()) == e))
}

/// Whether `brush` paints the content background.
fn is_background_brush(dc: HDC, brush: HBRUSH) -> bool {
    // A system colour index + 1 stands for its brush.
    if brush.0 as usize == COLOR_WINDOW.0 as usize + 1 {
        return true;
    }
    // SAFETY: plain query.
    let color = if HGDIOBJ(brush.0) == unsafe { GetStockObject(DC_BRUSH) } {
        // SAFETY: plain query on the caller's DC.
        unsafe { GetDCBrushColor(dc) }
    } else {
        let mut info = LOGBRUSH::default();
        let size = size_of::<LOGBRUSH>() as i32;
        // SAFETY: `info` is a LOGBRUSH of `size` bytes.
        let read = unsafe { GetObjectW(HGDIOBJ(brush.0), size, Some((&raw mut info).cast())) };
        if read != size || info.lbStyle != BS_SOLID {
            return false;
        }
        info.lbColor
    };
    state::is_background(color.0)
}

fn black() -> HBRUSH {
    // SAFETY: stock objects are never freed.
    HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0)
}

/// A 1×1 pattern of opaque black with only the alpha bits set, made once
/// and kept for the process.
fn alpha_brush() -> Option<HBRUSH> {
    #[repr(C)]
    struct PackedDib {
        header: BITMAPINFOHEADER,
        bits: [u32; 1],
    }
    static BRUSH: OnceLock<usize> = OnceLock::new();
    let brush = *BRUSH.get_or_init(|| {
        let dib = PackedDib {
            header: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 1,
                biHeight: 1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bits: [0xff00_0000],
        };
        // SAFETY: a packed DIB (header then pixels); GDI copies it.
        unsafe { CreateDIBPatternBrushPt((&raw const dib).cast(), DIB_RGB_COLORS) }.0 as usize
    });
    (brush != 0).then(|| HBRUSH(std::ptr::without_provenance_mut(brush)))
}

/// Sets the area's alpha to opaque, keeping its colours and the DC's
/// clipping.
fn make_opaque(dc: HDC, x: i32, y: i32, w: i32, h: i32) {
    let Some(brush) = alpha_brush() else {
        return;
    };
    // SAFETY: selects a live brush into the caller's DC and back.
    unsafe {
        let previous = SelectObject(dc, HGDIOBJ(brush.0));
        if previous.is_invalid() {
            return;
        }
        let _ = PatBlt(dc, x, y, w, h, DEST_OR_PATTERN);
        SelectObject(dc, previous);
    }
}

/// `FillRect` as the hook does it, for our own fills too.
pub(super) fn fill(dc: HDC, rect: *const RECT, brush: HBRUSH) -> i32 {
    // SAFETY (each call): the caller's DC, rectangle and brush.
    let Some(window) = state::glass_window_of(dc) else {
        return unsafe { FillRect(dc, rect, brush) };
    };
    seen(Seen::FillRect);
    if is_opaque_fill(window) {
        let result = unsafe { FillRect(dc, rect, brush) };
        // SAFETY: the caller passes a valid rectangle or null.
        if let (true, Some(r)) = (result != 0, unsafe { rect.as_ref() }) {
            make_opaque(dc, r.left, r.top, r.right - r.left, r.bottom - r.top);
        }
        return result;
    }
    let brush = if is_background_brush(dc, brush) {
        black()
    } else {
        brush
    };
    unsafe { FillRect(dc, rect, brush) }
}

pub(super) unsafe extern "system" fn fill_rect(dc: HDC, rect: *const RECT, brush: HBRUSH) -> i32 {
    hook_guard("FillRect hook", || fill(dc, rect, brush))
        // SAFETY: the caller's arguments, passed on unchanged.
        .unwrap_or_else(|| unsafe { FillRect(dc, rect, brush) })
}

pub(super) unsafe extern "system" fn pat_blt(
    dc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    rop: ROP_CODE,
) -> BOOL {
    // SAFETY (each call): the caller's arguments.
    let real = || unsafe { PatBlt(dc, x, y, w, h, rop) };
    hook_guard("PatBlt hook", || {
        let window = if rop == PATCOPY {
            state::glass_window_of(dc)
        } else {
            None
        };
        let Some(window) = window else {
            return real();
        };
        seen(Seen::PatBlt);
        if is_opaque_fill(window) {
            let result = real();
            if result.as_bool() {
                make_opaque(dc, x, y, w, h);
            }
            return result;
        }
        // SAFETY: plain query on the caller's DC.
        let brush = HBRUSH(unsafe { GetCurrentObject(dc, OBJ_BRUSH) }.0);
        if !is_background_brush(dc, brush) {
            return real();
        }
        // SAFETY: selects a stock brush into the caller's DC and back.
        unsafe {
            let previous = SelectObject(dc, HGDIOBJ(black().0));
            let result = real();
            SelectObject(dc, previous);
            result
        }
    })
    .unwrap_or_else(real)
}

/// Fills `rect` with `color` at `alpha` (premultiplied, through a
/// buffered paint).
fn fill_with_alpha(dc: HDC, rect: &RECT, color: COLORREF, alpha: u8) -> bool {
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let params = BP_PAINTPARAMS {
        cbSize: size_of::<BP_PAINTPARAMS>() as u32,
        pBlendFunction: &blend,
        ..Default::default()
    };
    let mut memory = HDC::default();
    // SAFETY: `rect`, `params` and `blend` live across the buffered paint.
    let buffer =
        unsafe { BeginBufferedPaint(dc, rect, BPBF_TOPDOWNDIB, Some(&params), &mut memory) };
    if buffer == 0 {
        return false;
    }
    let scale = |c: u8| u32::from(c) * u32::from(alpha) / 255;
    let (r, g, b) = gdi_glass::rgb(color.0);
    // SAFETY: a brush made here and deleted after use; the buffer's DC.
    unsafe {
        let brush = CreateSolidBrush(COLORREF(scale(r) | (scale(g) << 8) | (scale(b) << 16)));
        FillRect(memory, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = BufferedPaintSetAlpha(buffer, Some(rect), alpha);
        let _ = EndBufferedPaint(buffer, true);
    }
    true
}

/// Patched in ExplorerFrame only, which draws the navigation pane divider.
pub(super) unsafe extern "system" fn polyline(dc: HDC, points: *const POINT, count: i32) -> BOOL {
    if points.is_null() || count <= 0 {
        // SAFETY: nothing to draw; GDI reports the bad call itself.
        return unsafe { Polyline(dc, &[]) };
    }
    // SAFETY (each call): the caller passes `count` points at `points`.
    let real = || unsafe { Polyline(dc, std::slice::from_raw_parts(points, count as usize)) };
    if count != 2 {
        return real();
    }
    hook_guard("Polyline hook", || {
        // SAFETY: the caller passes `count` (2) points.
        let (a, b) = unsafe { (*points, *points.add(1)) };
        if state::glass_window_of(dc).is_none() {
            return real();
        }
        let mut pen = LOGPEN::default();
        let size = size_of::<LOGPEN>() as i32;
        // SAFETY: `pen` is a LOGPEN of `size` bytes.
        let read = unsafe {
            GetObjectW(
                GetCurrentObject(dc, OBJ_PEN),
                size,
                Some((&raw mut pen).cast()),
            )
        };
        if read != size || pen.lopnStyle != PS_SOLID.0 as u32 {
            return real();
        }
        let Some(rect) = gdi_glass::divider_rect((a.x, a.y), (b.x, b.y), pen.lopnWidth.x) else {
            return real();
        };
        seen(Seen::Divider);
        if fill_with_alpha(dc, &from_rect(rect), pen.lopnColor, DIVIDER_ALPHA) {
            BOOL(1)
        } else {
            real()
        }
    })
    .unwrap_or_else(real)
}
