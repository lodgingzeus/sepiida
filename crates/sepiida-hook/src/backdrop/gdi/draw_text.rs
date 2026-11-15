//! `DrawTextW`/`DrawTextExW` over the glass. They reach `ExtTextOutW`
//! inside gdi32 (its language layout code), which no import table sees,
//! so the text is drawn here as a white mask (`mask`) and given the text
//! colour with alpha. The nav pane, file list and status bar draw their
//! text this way, directly or through uxtheme's `DrawThemeText`.

use std::sync::OnceLock;

use sepiida_core::shell::gdi_glass;
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            DRAWTEXTPARAMS, DT_CALCRECT, DT_MODIFYSTRING, DT_NOCLIP, GetBkColor, GetBkMode,
            GetClipBox, GetTextAlign, HDC, OPAQUE, SetTextAlign, TEXT_ALIGN_OPTIONS,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    },
    core::{PCWSTR, PWSTR, s, w},
};

use super::{
    fill::{from_rect, to_rect},
    install::{Seen, hook_guard, seen},
    mask::draw_masked,
    state,
};

/// `GetClipBox`: no region or an error.
const NULLREGION: i32 = 1;
const ERROR: i32 = 0;

type DrawTextExFn =
    unsafe extern "system" fn(HDC, PWSTR, i32, *mut RECT, u32, *const DRAWTEXTPARAMS) -> i32;

/// user32's own `DrawTextExW`; our module's imports are never patched, but
/// the export is taken by name so every path here calls the same one.
fn real() -> Option<DrawTextExFn> {
    static REAL: OnceLock<Option<DrawTextExFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        // SAFETY: looks up a loaded module's export; its documented
        // signature.
        unsafe {
            let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
            GetProcAddress(user32, s!("DrawTextExW"))
                .map(|f| std::mem::transmute::<_, DrawTextExFn>(f))
        }
    })
}

fn call_real(
    dc: HDC,
    text: PWSTR,
    count: i32,
    rect: *mut RECT,
    format: u32,
    params: *const DRAWTEXTPARAMS,
) -> i32 {
    // SAFETY: the caller's arguments, passed on unchanged.
    real().map_or(0, |f| unsafe { f(dc, text, count, rect, format, params) })
}

/// Draws with alpha where `dc` belongs to a glass window; `None` where the
/// real call should be made instead.
fn with_alpha(
    dc: HDC,
    text: PWSTR,
    count: i32,
    rect: *mut RECT,
    format: u32,
    params: *const DRAWTEXTPARAMS,
) -> Option<i32> {
    if format & DT_CALCRECT.0 != 0 || text.is_null() || count == 0 {
        return None;
    }
    state::glass_window_of(dc)?;
    seen(Seen::DrawText);
    // SAFETY: the caller passes a valid rectangle or null.
    let bounds = unsafe { rect.as_ref() }.map(to_rect)?;
    // SAFETY (each): plain queries on the caller's DC.
    let opaque = unsafe { GetBkMode(dc) } == OPAQUE.0 as i32;
    // An opaque background other than the content's would lose its alpha
    // in the mask: left to GDI. The content background is dropped, as
    // the fill hooks make it transparent.
    if opaque && !state::is_background(unsafe { GetBkColor(dc) }.0) {
        return None;
    }
    let extent = (format & DT_NOCLIP.0 != 0).then(|| {
        let mut calculated = from_rect(bounds);
        let format = (format | DT_CALCRECT.0) & !DT_MODIFYSTRING.0;
        call_real(dc, text, count, &mut calculated, format, params);
        to_rect(&calculated)
    });
    let mut clip = RECT::default();
    // SAFETY: plain query on the caller's DC; `clip` is ours.
    let region = unsafe { GetClipBox(dc, &mut clip) }.0;
    if region == NULLREGION {
        // Nothing is visible; GDI does the rest (DT_CALCRECT effects).
        return Some(call_real(dc, text, count, rect, format, params));
    }
    let clip = (region != ERROR).then(|| to_rect(&clip));
    let area = gdi_glass::draw_text_area(bounds, extent, clip)?;
    // SAFETY: plain query on the caller's DC.
    let align = unsafe { GetTextAlign(dc) }.0;
    let result = draw_masked(dc, area, |memory| {
        // SAFETY: the buffer's DC.
        unsafe { SetTextAlign(memory, TEXT_ALIGN_OPTIONS(align)) };
        // Inside, nothing of ours may act on the mask.
        state::with_uxtheme_text(|| call_real(memory, text, count, rect, format, params))
    })?;
    seen(Seen::DrawTextAlpha);
    Some(result)
}

/// `DrawTextExW` as the hook does it; also used for uxtheme's glowless
/// text.
pub(super) fn draw_text_ex(
    dc: HDC,
    text: PWSTR,
    count: i32,
    rect: *mut RECT,
    format: u32,
    params: *const DRAWTEXTPARAMS,
) -> i32 {
    with_alpha(dc, text, count, rect, format, params)
        .unwrap_or_else(|| call_real(dc, text, count, rect, format, params))
}

pub(super) unsafe extern "system" fn draw_text_ex_hook(
    dc: HDC,
    text: PWSTR,
    count: i32,
    rect: *mut RECT,
    format: u32,
    params: *const DRAWTEXTPARAMS,
) -> i32 {
    hook_guard("DrawTextExW hook", || {
        draw_text_ex(dc, text, count, rect, format, params)
    })
    .unwrap_or_else(|| call_real(dc, text, count, rect, format, params))
}

/// `DrawTextW` is `DrawTextExW` without parameters; the text is only
/// written to with `DT_MODIFYSTRING`, as by the real function.
pub(super) unsafe extern "system" fn draw_text(
    dc: HDC,
    text: PCWSTR,
    count: i32,
    rect: *mut RECT,
    format: u32,
) -> i32 {
    let text = PWSTR(text.0.cast_mut());
    let params = std::ptr::null();
    hook_guard("DrawTextW hook", || {
        draw_text_ex(dc, text, count, rect, format, params)
    })
    .unwrap_or_else(|| call_real(dc, text, count, rect, format, params))
}
