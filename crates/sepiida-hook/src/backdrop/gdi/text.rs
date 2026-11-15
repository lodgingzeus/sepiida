//! Text over the glass: `ExtTextOutW` on a glass window is drawn as a
//! white coverage mask in a buffered paint and turned into the text colour
//! with alpha. Text uxtheme composites itself (`DrawThemeTextEx` with
//! `DTT_COMPOSITED`) already has alpha and is left alone; glowless
//! `DrawTextWithGlow` is drawn as plain text, which then comes through
//! `ExtTextOutW`.

use std::sync::OnceLock;

use sepiida_core::shell::gdi_glass::{self, Rect, Run, text as run_text};
use windows::{
    Win32::{
        Foundation::{COLORREF, E_FAIL, LPARAM, RECT, SIZE},
        Graphics::Gdi::{
            COLOR_HIGHLIGHT, CreateSolidBrush, DT_CALCRECT, DT_MODIFYSTRING, DeleteObject,
            ETO_OPTIONS, ExtTextOutW, FillRect, GetBkColor, GetSysColor, GetSysColorBrush,
            GetTextAlign, GetTextExtentPoint32W, GetTextExtentPointI, GetTextMetricsW, HDC,
            HGDIOBJ, SetBkColor, SetTextAlign, SetTextColor, TA_UPDATECP, TEXT_ALIGN_OPTIONS,
            TEXTMETRICW,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::Controls::{
            BP_PAINTPARAMS, BPBF_TOPDOWNDIB, BeginBufferedPaint, BufferedPaintSetAlpha,
            DTT_COMPOSITED, DTTOPTS, EndBufferedPaint, HTHEME,
        },
    },
    core::{HRESULT, PCSTR, PCWSTR, PWSTR, s, w},
};
use windows_core::BOOL;

use super::{
    draw_text,
    fill::{self, is_opaque_fill, to_rect},
    install::{DRAW_TEXT_WITH_GLOW, Seen, hook_guard, seen},
    mask::draw_masked,
    state,
};

/// TA_BASELINE: `text_rect` needs the ascent only then.
const TA_BASELINE: u32 = 24;
/// More characters than any real call draws; a larger count is passed on.
const MAX_CHARS: u32 = 1 << 16;

type DrawThemeTextExFn = unsafe extern "system" fn(
    HTHEME,
    HDC,
    i32,
    i32,
    PCWSTR,
    i32,
    u32,
    *mut RECT,
    *const DTTOPTS,
) -> HRESULT;
type DrawTextCallback =
    Option<unsafe extern "system" fn(HDC, PWSTR, i32, *mut RECT, u32, LPARAM) -> i32>;
type DrawTextWithGlowFn = unsafe extern "system" fn(
    HDC,
    PWSTR,
    u32,
    *mut RECT,
    u32,
    COLORREF,
    COLORREF,
    u32,
    u32,
    BOOL,
    DrawTextCallback,
    LPARAM,
) -> HRESULT;

/// An export of a loaded system DLL.
fn export(module: PCWSTR, name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY: looks up a loaded module and one of its exports.
    unsafe { GetProcAddress(GetModuleHandleW(module).ok()?, name) }
}

fn real_draw_theme_text_ex() -> Option<DrawThemeTextExFn> {
    static REAL: OnceLock<Option<DrawThemeTextExFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        // SAFETY: the export has had this signature since Windows Vista.
        export(w!("uxtheme.dll"), s!("DrawThemeTextEx"))
            .map(|f| unsafe { std::mem::transmute::<_, DrawThemeTextExFn>(f) })
    })
}

fn real_draw_text_with_glow() -> Option<DrawTextWithGlowFn> {
    static REAL: OnceLock<Option<DrawTextWithGlowFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        let ordinal = PCSTR(std::ptr::without_provenance(usize::from(
            DRAW_TEXT_WITH_GLOW,
        )));
        // SAFETY: the signature the mod uses for this ordinal.
        export(w!("uxtheme.dll"), ordinal)
            .map(|f| unsafe { std::mem::transmute::<_, DrawTextWithGlowFn>(f) })
    })
}

/// One `ExtTextOutW` call's arguments.
#[derive(Clone, Copy)]
pub(super) struct TextOut {
    pub dc: HDC,
    pub x: i32,
    pub y: i32,
    pub options: u32,
    pub rect: *const RECT,
    pub text: PCWSTR,
    pub count: u32,
    pub dx: *const i32,
}

impl TextOut {
    fn real(&self, dc: HDC, options: u32) -> BOOL {
        let rect = (!self.rect.is_null()).then_some(self.rect);
        let dx = (!self.dx.is_null()).then_some(self.dx);
        // SAFETY: the caller's arguments, on its DC or our buffer's.
        unsafe {
            ExtTextOutW(
                dc,
                self.x,
                self.y,
                ETO_OPTIONS(options),
                rect,
                self.text,
                self.count,
                dx,
            )
        }
    }

    fn rect(&self) -> Option<Rect> {
        // SAFETY: the caller passes a valid rectangle or null.
        unsafe { self.rect.as_ref() }.map(to_rect)
    }

    /// Where the text lands on the DC, if anywhere.
    fn bounds(&self, align: u32) -> Option<Rect> {
        let count = self.count as usize;
        // SAFETY: the caller passes `count` characters (or glyph indices).
        let chars = unsafe { std::slice::from_raw_parts(self.text.0, count) };
        let mut size = SIZE::default();
        // SAFETY: the caller's DC; `size` is ours.
        let measured = unsafe {
            if self.options & run_text::ETO_GLYPH_INDEX != 0 {
                GetTextExtentPointI(self.dc, chars, &mut size)
            } else {
                GetTextExtentPoint32W(self.dc, chars, &mut size)
            }
        };
        if !measured.as_bool() {
            return None;
        }
        let pdy = self.options & run_text::ETO_PDY != 0;
        let (width, height) = if self.dx.is_null() {
            (size.cx, size.cy)
        } else {
            let len = if pdy { count * 2 } else { count };
            // SAFETY: the caller passes one advance (two with ETO_PDY) per
            // character.
            let dx = unsafe { std::slice::from_raw_parts(self.dx, len) };
            gdi_glass::extent_with_advances(size.cx, size.cy, dx, pdy)
        };
        let ascent = if align & TA_BASELINE == TA_BASELINE {
            let mut metrics = TEXTMETRICW::default();
            // SAFETY: the caller's DC; `metrics` is ours.
            if unsafe { GetTextMetricsW(self.dc, &mut metrics) }.as_bool() {
                metrics.tmAscent
            } else {
                0
            }
        } else {
            0
        };
        let run = Run {
            x: self.x,
            y: self.y,
            options: self.options,
            align,
            width,
            height,
            ascent,
        };
        gdi_glass::text_rect(&run, self.rect())
    }
}

/// The opaque background of an `ETO_OPAQUE` call: the selection highlight
/// is made opaque, any other colour filled as the FillRect hook would.
fn paint_background(dc: HDC, rect: *const RECT) -> bool {
    // SAFETY (each call): the caller's DC and rectangle; brushes made here
    // are deleted after use.
    unsafe {
        let color = GetBkColor(dc);
        if color.0 != GetSysColor(COLOR_HIGHLIGHT) {
            let brush = CreateSolidBrush(color);
            fill::fill(dc, rect, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            return true;
        }
        let params = BP_PAINTPARAMS {
            cbSize: size_of::<BP_PAINTPARAMS>() as u32,
            ..Default::default()
        };
        let mut memory = HDC::default();
        let buffer = BeginBufferedPaint(dc, rect, BPBF_TOPDOWNDIB, Some(&params), &mut memory);
        if buffer == 0 {
            return false;
        }
        FillRect(memory, rect, GetSysColorBrush(COLOR_HIGHLIGHT));
        let _ = BufferedPaintSetAlpha(buffer, Some(rect), 255);
        let _ = EndBufferedPaint(buffer, true);
        true
    }
}

/// Draws the call's text with alpha; `None` where the real call should be
/// made instead.
fn with_alpha(call: &TextOut) -> Option<BOOL> {
    let dc = call.dc;
    // SAFETY: plain query on the caller's DC.
    let align = unsafe { GetTextAlign(dc) }.0;
    if call.text.is_null()
        || call.count == 0
        || call.count > MAX_CHARS
        || call.options == 0
        || align & TA_UPDATECP.0 != 0
    {
        return None;
    }
    let boxed = run_text::ETO_OPAQUE | run_text::ETO_CLIPPED;
    if call.options & boxed != 0 && call.rect().is_none_or(|r| r.is_empty()) {
        return None;
    }
    let bounds = call.bounds(align)?;
    if call.options & run_text::ETO_OPAQUE != 0 && !paint_background(dc, call.rect) {
        return None;
    }
    let result = draw_masked(dc, bounds, |memory| {
        // SAFETY: the buffer's DC.
        unsafe { SetTextAlign(memory, TEXT_ALIGN_OPTIONS(align)) };
        call.real(memory, call.options & !run_text::ETO_OPAQUE)
    })?;
    seen(Seen::TextOutAlpha);
    Some(result)
}

fn text_out(call: &TextOut) -> BOOL {
    let Some(window) = state::glass_window_of(call.dc) else {
        return call.real(call.dc, call.options);
    };
    seen(Seen::TextOut);
    let dc = call.dc;
    // SAFETY: plain query on the caller's DC.
    let background = call.options & run_text::ETO_OPAQUE != 0
        && state::is_background(unsafe { GetBkColor(dc) }.0)
        && !is_opaque_fill(window);
    // SAFETY: changes the caller's DC background colour, put back below.
    let previous = background.then(|| unsafe { SetBkColor(dc, COLORREF(0)) });
    let result = with_alpha(call).unwrap_or_else(|| call.real(dc, call.options));
    if let Some(previous) = previous {
        // SAFETY: as above.
        unsafe { SetBkColor(dc, previous) };
    }
    result
}

#[allow(clippy::too_many_arguments, reason = "ExtTextOutW's signature")]
pub(super) unsafe extern "system" fn ext_text_out(
    dc: HDC,
    x: i32,
    y: i32,
    options: u32,
    rect: *const RECT,
    text: PCWSTR,
    count: u32,
    dx: *const i32,
) -> BOOL {
    let call = TextOut {
        dc,
        x,
        y,
        options,
        rect,
        text,
        count,
        dx,
    };
    hook_guard("ExtTextOutW hook", || text_out(&call)).unwrap_or_else(|| call.real(dc, options))
}

#[allow(clippy::too_many_arguments, reason = "DrawThemeTextEx's signature")]
pub(super) unsafe extern "system" fn draw_theme_text_ex(
    theme: HTHEME,
    dc: HDC,
    part: i32,
    state_id: i32,
    text: PCWSTR,
    count: i32,
    flags: u32,
    rect: *mut RECT,
    options: *const DTTOPTS,
) -> HRESULT {
    let Some(real) = real_draw_theme_text_ex() else {
        return E_FAIL;
    };
    // SAFETY (each call): the caller's arguments, passed on unchanged.
    let call = || unsafe { real(theme, dc, part, state_id, text, count, flags, rect, options) };
    let composited = hook_guard("DrawThemeTextEx hook", || {
        // SAFETY: the caller passes a valid DTTOPTS or null.
        let composited =
            unsafe { options.as_ref() }.is_some_and(|o| o.dwFlags.0 & DTT_COMPOSITED.0 != 0);
        if composited && state::glass_window_of(dc).is_some() {
            seen(Seen::UxthemeText);
        }
        composited
    })
    .unwrap_or(false);
    if !composited {
        return call();
    }
    state::with_uxtheme_text(call)
}

#[allow(clippy::too_many_arguments, reason = "DrawTextWithGlow's signature")]
pub(super) unsafe extern "system" fn draw_text_with_glow(
    dc: HDC,
    text: PWSTR,
    count: u32,
    rect: *mut RECT,
    flags: u32,
    text_color: COLORREF,
    glow_color: COLORREF,
    glow_radius: u32,
    glow_intensity: u32,
    premultiply: BOOL,
    callback: DrawTextCallback,
    lparam: LPARAM,
) -> HRESULT {
    let Some(real) = real_draw_text_with_glow() else {
        return E_FAIL;
    };
    let original = || {
        state::with_uxtheme_text(|| {
            // SAFETY: the caller's arguments, passed on unchanged.
            unsafe {
                real(
                    dc,
                    text,
                    count,
                    rect,
                    flags,
                    text_color,
                    glow_color,
                    glow_radius,
                    glow_intensity,
                    premultiply,
                    callback,
                    lparam,
                )
            }
        })
    };
    let plain = hook_guard("DrawTextWithGlow hook", || {
        // SAFETY: the caller passes a valid rectangle or null.
        let rect_empty =
            unsafe { rect.as_ref() }.is_none_or(|r| r.right == r.left || r.bottom == r.top);
        if glow_radius > 0
            || state::glass_window_of(dc).is_none()
            || (rect_empty && flags & DT_CALCRECT.0 == 0)
        {
            return None;
        }
        seen(Seen::GlowText);
        Some(draw_plain(
            dc, text, count, rect, flags, text_color, callback, lparam,
        ))
    })
    .flatten();
    plain.unwrap_or_else(original)
}

/// Glowless text as plain GDI text; its alpha comes from the ExtTextOutW
/// hook.
#[allow(clippy::too_many_arguments, reason = "DrawTextWithGlow's arguments")]
fn draw_plain(
    dc: HDC,
    text: PWSTR,
    count: u32,
    rect: *mut RECT,
    flags: u32,
    color: COLORREF,
    callback: DrawTextCallback,
    lparam: LPARAM,
) -> HRESULT {
    // SAFETY (block): the caller's DC, text and rectangle; the colours are
    // put back below.
    unsafe {
        let previous_text = SetTextColor(dc, color);
        let previous_back = SetBkColor(dc, COLORREF(0));
        // DrawTextWithGlow takes the count unsigned; -1 (NUL-terminated)
        // arrives as u32::MAX and goes on as -1.
        let count = count as i32;
        let result = match callback {
            Some(callback) => HRESULT(callback(dc, text, count, rect, flags, lparam)),
            // Our DrawText path, which gives the text alpha.
            None => {
                let format = flags & !DT_MODIFYSTRING.0;
                if draw_text::draw_text_ex(dc, text, count, rect, format, std::ptr::null()) != 0 {
                    HRESULT(0)
                } else {
                    E_FAIL
                }
            }
        };
        SetTextColor(dc, previous_text);
        SetBkColor(dc, previous_back);
        result
    }
}
