//! Theme parts over the glass (`DrawThemeBackground(Ex)`): scroll bars,
//! the column header and the pane separator of a glass window are drawn
//! as `sepiida_core::shell::gdi_glass::parts` says, every other part by
//! uxtheme. The bitmaps are made once per DPI and kept for the process.

use std::sync::{Mutex, MutexGuard, OnceLock};

use sepiida_core::shell::gdi_glass::{
    Rect,
    parts::{self, Bitmap, NineGrid, Paint},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, RECT},
        Graphics::Gdi::{
            AC_SRC_ALPHA, AC_SRC_OVER, AlphaBlend, BI_RGB, BITMAPINFO, BITMAPINFOHEADER,
            BLACK_BRUSH, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
            DeleteDC, DeleteObject, FillRect, GetCurrentObject, GetStockObject, HBRUSH, HDC,
            IntersectClipRect, OBJ_BITMAP, RestoreDC, SaveDC, SelectObject,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Controls::{DTBG_CLIPRECT, DTBGOPTS, HTHEME},
            HiDpi::{GetDpiForSystem, GetDpiForWindow},
        },
    },
    core::{HRESULT, PCSTR, PWSTR, s, w},
};

use crate::{imports, log::log};

use super::{
    fill::to_rect,
    install::{Seen, hook_guard, seen},
    state,
};

/// uxtheme's `GetThemeClass`, exported by ordinal only.
const GET_THEME_CLASS: u16 = 74;

type DrawBackgroundFn =
    unsafe extern "system" fn(HTHEME, HDC, i32, i32, *const RECT, *const RECT) -> HRESULT;
type DrawBackgroundExFn =
    unsafe extern "system" fn(HTHEME, HDC, i32, i32, *const RECT, *const DTBGOPTS) -> HRESULT;
type GetThemeClassFn = unsafe extern "system" fn(HTHEME, PWSTR, i32) -> HRESULT;

fn uxtheme(name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY: looks up a loaded module's export by name or ordinal.
    unsafe { GetProcAddress(GetModuleHandleW(w!("uxtheme.dll")).ok()?, name) }
}

/// uxtheme's theme-part drawing, for modules that stored pointers to it
/// instead of importing it (DirectUI draws the file list's column
/// header and background this way).
pub(super) fn stored_functions() -> Vec<imports::Stored> {
    let mut functions = Vec::new();
    if let Some(real) = real_background() {
        functions.push(imports::Stored {
            name: "DrawThemeBackground",
            real: real as usize,
            address: draw_background as *const () as usize,
        });
    }
    if let Some(real) = real_background_ex() {
        functions.push(imports::Stored {
            name: "DrawThemeBackgroundEx",
            real: real as usize,
            address: draw_background_ex as *const () as usize,
        });
    }
    functions
}

fn real_background() -> Option<DrawBackgroundFn> {
    static REAL: OnceLock<Option<DrawBackgroundFn>> = OnceLock::new();
    // SAFETY: the export's documented signature.
    *REAL.get_or_init(|| {
        uxtheme(s!("DrawThemeBackground")).map(|f| unsafe { std::mem::transmute(f) })
    })
}

fn real_background_ex() -> Option<DrawBackgroundExFn> {
    static REAL: OnceLock<Option<DrawBackgroundExFn>> = OnceLock::new();
    // SAFETY: the export's documented signature.
    *REAL.get_or_init(|| {
        uxtheme(s!("DrawThemeBackgroundEx")).map(|f| unsafe { std::mem::transmute(f) })
    })
}

fn get_theme_class() -> Option<GetThemeClassFn> {
    static REAL: OnceLock<Option<GetThemeClassFn>> = OnceLock::new();
    *REAL.get_or_init(|| {
        let ordinal = PCSTR(std::ptr::without_provenance(usize::from(GET_THEME_CLASS)));
        // SAFETY: the signature the mod uses for this ordinal.
        uxtheme(ordinal).map(|f| unsafe { std::mem::transmute(f) })
    })
}

/// A part bitmap selected into a memory DC of its own.
struct Cached {
    dpi: u32,
    kind: u8,
    dc: usize,
    width: i32,
    height: i32,
    margins: [i32; 4],
}

static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

fn cache() -> MutexGuard<'static, Vec<Cached>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A memory DC with `bitmap` in a 32-bit DIB section; `None` on failure.
/// The DC and bitmap are kept by the caller (or freed by `free_dc`).
fn make_dc(bitmap: &Bitmap) -> Option<HDC> {
    let (width, height) = (
        i32::try_from(bitmap.width).ok()?,
        i32::try_from(bitmap.height).ok()?,
    );
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY (block): a new DC and DIB section; the pixels are copied into
    // the section's `width * height` 32-bit pixels.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let mut bits = std::ptr::null_mut();
        let Ok(section) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(dc);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(section.into());
            let _ = DeleteDC(dc);
            return None;
        }
        std::ptr::copy_nonoverlapping(
            bitmap.pixels.as_ptr(),
            bits.cast::<u32>(),
            bitmap.pixels.len(),
        );
        SelectObject(dc, section.into());
        Some(dc)
    }
}

fn free_dc(dc: HDC) {
    // SAFETY: a DC made by `make_dc`; its bitmap is freed with it.
    unsafe {
        let bitmap = GetCurrentObject(dc, OBJ_BITMAP);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bitmap);
    }
}

const BLEND: BLENDFUNCTION = BLENDFUNCTION {
    BlendOp: AC_SRC_OVER as u8,
    BlendFlags: 0,
    SourceConstantAlpha: 255,
    AlphaFormat: AC_SRC_ALPHA as u8,
};

fn blend(dc: HDC, d: Rect, source: HDC, s: Rect) {
    // SAFETY: both DCs are live; the rectangles lie within them.
    let _ = unsafe {
        AlphaBlend(
            dc,
            d.left,
            d.top,
            d.width(),
            d.height(),
            source,
            s.left,
            s.top,
            s.width(),
            s.height(),
            BLEND,
        )
    };
}

/// Blends the cached bitmap `kind` at `dpi` (made by `make` the first
/// time) stretched over `rect`.
fn blend_cached(dc: HDC, rect: Rect, dpi: u32, kind: u8, make: impl FnOnce() -> NineGrid) -> bool {
    let mut cache = cache();
    if !cache.iter().any(|c| c.dpi == dpi && c.kind == kind) {
        let grid = make();
        let Some(made) = make_dc(&grid.bitmap) else {
            return false;
        };
        cache.push(Cached {
            dpi,
            kind,
            dc: made.0 as usize,
            width: grid.bitmap.width as i32,
            height: grid.bitmap.height as i32,
            margins: grid.margins,
        });
    }
    let Some(cached) = cache.iter().find(|c| c.dpi == dpi && c.kind == kind) else {
        return false;
    };
    let source = HDC(std::ptr::without_provenance_mut(cached.dc));
    // Blended while the cache is locked: one thread at a time uses a DC.
    for (d, s) in parts::nine_grid(rect, cached.width, cached.height, cached.margins) {
        blend(dc, d, source, s);
    }
    true
}

fn clear(dc: HDC, rect: &RECT) {
    // SAFETY: a stock brush on the caller's DC and rectangle.
    unsafe { FillRect(dc, rect, HBRUSH(GetStockObject(BLACK_BRUSH).0)) };
}

fn dpi_of(window: HWND) -> u32 {
    // SAFETY: plain queries.
    match unsafe { GetDpiForWindow(window) } {
        0 => unsafe { GetDpiForSystem() },
        dpi => dpi,
    }
}

/// Theme classes and parts drawn on glass windows, each logged once (at
/// most `NOTED_MAX`), to find what else needs a look on the glass.
fn note_class(class: &str, part: i32, ours: bool) {
    const NOTED_MAX: usize = 64;
    static NOTED: Mutex<Vec<(String, i32)>> = Mutex::new(Vec::new());
    let mut noted = NOTED.lock().unwrap_or_else(|e| e.into_inner());
    if noted.len() >= NOTED_MAX || noted.iter().any(|(c, p)| c == class && *p == part) {
        return;
    }
    noted.push((class.to_owned(), part));
    let by = if ours { "ours" } else { "left to uxtheme" };
    log!("gdi: theme class {class} part {part} on a glass window ({by})");
}

/// The class name of `theme`, if uxtheme tells it.
fn class_of(theme: HTHEME) -> Option<String> {
    let get = get_theme_class()?;
    let mut name = [0u16; 64];
    // SAFETY: the buffer and its length are ours.
    unsafe { get(theme, PWSTR(name.as_mut_ptr()), name.len() as i32) }
        .ok()
        .ok()?;
    let length = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..length]))
}

/// Paints the part if it is ours on a glass window; returns whether it
/// did.
fn paint(
    theme: HTHEME,
    dc: HDC,
    part: i32,
    state_id: i32,
    rect: *const RECT,
    clip: *const RECT,
) -> bool {
    // SAFETY: the caller passes a valid rectangle or null.
    let Some(rect) = (unsafe { rect.as_ref() }) else {
        return false;
    };
    let Some(window) = state::glass_window_of(dc) else {
        return false;
    };
    let Some(class) = class_of(theme) else {
        return false;
    };
    let paint = parts::paint_for(&class, part, state_id);
    note_class(&class, part, paint.is_some());
    let Some(paint) = paint else {
        return false;
    };
    seen(Seen::ThemePart);
    paint_part(dc, dpi_of(window), paint, rect, clip)
}

/// Draws `paint` over `rect` at `dpi`, within `clip` if given.
pub(super) fn paint_part(dc: HDC, dpi: u32, paint: Paint, rect: &RECT, clip: *const RECT) -> bool {
    // SAFETY: saved and restored around the clip below.
    let saved = unsafe { clip.as_ref() }.map(|c| unsafe {
        let saved = SaveDC(dc);
        IntersectClipRect(dc, c.left, c.top, c.right, c.bottom);
        saved
    });
    let area = to_rect(rect);
    clear(dc, rect);
    let painted = match paint {
        Paint::Clear => true,
        Paint::Thumb { horizontal, hot } => {
            let kind = u8::from(horizontal) * 2 + u8::from(hot);
            blend_cached(dc, area, dpi, kind, || parts::thumb(horizontal, hot, dpi))
        }
        Paint::HeaderItem { hot } => blend_cached(dc, area, dpi, 4 + u8::from(hot), || {
            parts::header_item(hot, dpi)
        }),
        Paint::Track => blend_cached(dc, area, dpi, 6, || parts::track(dpi)),
        Paint::Arrow(direction, look) => {
            let (w, h) = (
                usize::try_from(area.width()).unwrap_or(0),
                usize::try_from(area.height()).unwrap_or(0),
            );
            match parts::arrow_bitmap(direction, look, w, h, dpi) {
                None => true,
                Some(bitmap) => match make_dc(&bitmap) {
                    Some(source) => {
                        blend(
                            dc,
                            area,
                            source,
                            Rect::new(0, 0, area.width(), area.height()),
                        );
                        free_dc(source);
                        true
                    }
                    None => false,
                },
            }
        }
    };
    if let Some(saved) = saved {
        // SAFETY: the state saved above.
        let _ = unsafe { RestoreDC(dc, saved) };
    }
    painted
}

pub(super) unsafe extern "system" fn draw_background(
    theme: HTHEME,
    dc: HDC,
    part: i32,
    state_id: i32,
    rect: *const RECT,
    clip: *const RECT,
) -> HRESULT {
    if hook_guard("DrawThemeBackground hook", || {
        paint(theme, dc, part, state_id, rect, clip)
    }) == Some(true)
    {
        return HRESULT(0);
    }
    // SAFETY: the caller's arguments, passed on unchanged.
    real_background().map_or(E_FAIL, |f| unsafe {
        f(theme, dc, part, state_id, rect, clip)
    })
}

pub(super) unsafe extern "system" fn draw_background_ex(
    theme: HTHEME,
    dc: HDC,
    part: i32,
    state_id: i32,
    rect: *const RECT,
    options: *const DTBGOPTS,
) -> HRESULT {
    let painted = hook_guard("DrawThemeBackgroundEx hook", || {
        // SAFETY: the caller passes valid options or null.
        let clip = unsafe { options.as_ref() }
            .filter(|o| o.dwFlags & DTBG_CLIPRECT != 0)
            .map_or(std::ptr::null(), |o| &raw const o.rcClip);
        paint(theme, dc, part, state_id, rect, clip)
    });
    if painted == Some(true) {
        return HRESULT(0);
    }
    // SAFETY: the caller's arguments, passed on unchanged.
    real_background_ex().map_or(E_FAIL, |f| unsafe {
        f(theme, dc, part, state_id, rect, options)
    })
}
