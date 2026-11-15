//! Text with alpha by way of a coverage mask: the caller's drawing is
//! repeated in white on black in a buffered paint over `bounds`, then
//! turned into the DC's text colour with alpha
//! (`gdi_glass::colorize_mask`) and blended onto the DC.

use sepiida_core::shell::gdi_glass::{self, Rect};
use windows::Win32::{
    Foundation::COLORREF,
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, DC_LAYOUT, GetCurrentObject, GetLayout,
        GetTextColor, HDC, OBJ_FONT, RGBQUAD, SelectObject, SetBkMode, SetLayout, SetTextColor,
        TRANSPARENT,
    },
    UI::Controls::{
        BP_PAINTPARAMS, BPBF_TOPDOWNDIB, BPPF_ERASE, BPPF_NOCLIP, BeginBufferedPaint,
        EndBufferedPaint, GetBufferedPaintBits,
    },
};

use super::fill::from_rect;

/// Runs `draw` on a buffer DC over `bounds` (same coordinates as `dc`,
/// with its font and layout, white transparent text), then blends the
/// result onto `dc` as its text colour with alpha. `None` if the buffer
/// could not be made or read; nothing is drawn on `dc` then.
pub(super) fn draw_masked<T>(dc: HDC, bounds: Rect, draw: impl FnOnce(HDC) -> T) -> Option<T> {
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let params = BP_PAINTPARAMS {
        cbSize: size_of::<BP_PAINTPARAMS>() as u32,
        dwFlags: BPPF_ERASE | BPPF_NOCLIP,
        pBlendFunction: &blend,
        ..Default::default()
    };
    let target = from_rect(bounds);
    let mut memory = HDC::default();
    // SAFETY: `target`, `params` and `blend` live across the buffered paint.
    let buffer =
        unsafe { BeginBufferedPaint(dc, &target, BPBF_TOPDOWNDIB, Some(&params), &mut memory) };
    if buffer == 0 {
        return None;
    }
    // SAFETY (block): the buffer's DC and the caller's font, selected back.
    let result = unsafe {
        let previous = SelectObject(memory, GetCurrentObject(dc, OBJ_FONT));
        SetLayout(memory, DC_LAYOUT(GetLayout(dc)));
        SetBkMode(memory, TRANSPARENT);
        SetTextColor(memory, COLORREF(0x00ff_ffff));
        let result = draw(memory);
        SelectObject(memory, previous);
        result
    };
    let mut pixels: *mut RGBQUAD = std::ptr::null_mut();
    let mut stride = 0;
    // SAFETY: the buffer is live; the outputs are ours.
    let bits = unsafe { GetBufferedPaintBits(buffer, &mut pixels, &mut stride) };
    let (Ok(()), Ok(stride), false) = (bits, usize::try_from(stride), pixels.is_null()) else {
        // SAFETY: ends the buffer without drawing it.
        let _ = unsafe { EndBufferedPaint(buffer, false) };
        return None;
    };
    let width = usize::try_from(bounds.width()).unwrap_or(0);
    let height = usize::try_from(bounds.height()).unwrap_or(0);
    // SAFETY: the buffer holds `height` rows of `stride` 32-bit pixels
    // covering `bounds`, and is ours until EndBufferedPaint.
    let slice = unsafe { std::slice::from_raw_parts_mut(pixels.cast::<u32>(), stride * height) };
    // SAFETY: plain query on the caller's DC.
    let color = unsafe { GetTextColor(dc) };
    gdi_glass::colorize_mask(slice, width, height, stride, color.0);
    // SAFETY: draws the buffer onto the caller's DC and frees it.
    let _ = unsafe { EndBufferedPaint(buffer, true) };
    Some(result)
}
