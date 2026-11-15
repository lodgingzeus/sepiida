//! Text with alpha: where `ExtTextOutW` puts its text, and how a white
//! coverage mask drawn by GDI becomes the text colour with alpha.

use std::sync::LazyLock;

use super::{ColorRef, rgb};

// `SetTextAlign` flags. TA_BASELINE and TA_CENTER are supersets of
// TA_BOTTOM and TA_RIGHT, so the fields are compared as a whole.
const TA_RIGHT: u32 = 2;
const TA_CENTER: u32 = 6;
const TA_BOTTOM: u32 = 8;
const TA_BASELINE: u32 = 24;
// `ExtTextOutW` options.
pub const ETO_OPAQUE: u32 = 0x2;
pub const ETO_CLIPPED: u32 = 0x4;
pub const ETO_GLYPH_INDEX: u32 = 0x10;
pub const ETO_PDY: u32 = 0x2000;

/// A GDI rectangle, right and bottom exclusive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    pub const fn width(&self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub const fn height(&self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    /// The overlap, empty (all zero) if there is none, as `IntersectRect`.
    pub fn intersect(&self, other: &Self) -> Self {
        let r = Self::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        if r.is_empty() { Self::default() } else { r }
    }

    /// The smallest rectangle holding both, as `UnionRect` (an empty one
    /// adds nothing).
    pub fn union(&self, other: &Self) -> Self {
        match (self.is_empty(), other.is_empty()) {
            (true, true) => Self::default(),
            (true, false) => *other,
            (false, true) => *self,
            (false, false) => Self::new(
                self.left.min(other.left),
                self.top.min(other.top),
                self.right.max(other.right),
                self.bottom.max(other.bottom),
            ),
        }
    }
}

/// One `ExtTextOutW` call, with what the DC says about its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub x: i32,
    pub y: i32,
    pub options: u32,
    /// `GetTextAlign`.
    pub align: u32,
    /// The text's extent (`GetTextExtentPoint`, widened by the advances).
    pub width: i32,
    pub height: i32,
    /// `TEXTMETRIC::tmAscent`, for baseline alignment.
    pub ascent: i32,
}

/// The extent `width` × `height` widened to the sum of the advances `dx`
/// (pairs of x and y advances with `ETO_PDY`), as GDI lays them out.
pub fn extent_with_advances(width: i32, height: i32, dx: &[i32], pdy: bool) -> (i32, i32) {
    let stride = if pdy { 2 } else { 1 };
    let mut sum_x: i32 = 0;
    let mut sum_y: i32 = 0;
    for advance in dx.chunks(stride) {
        sum_x = sum_x.saturating_add(advance[0]);
        if let Some(&y) = advance.get(1) {
            sum_y = sum_y.saturating_add(y);
        }
    }
    let abs_y = i32::try_from(sum_y.unsigned_abs()).unwrap_or(i32::MAX);
    (width.max(sum_x), height.saturating_add(abs_y))
}

/// Where the run's text lands, clipped to `rect` with `ETO_CLIPPED` and
/// grown to it with `ETO_OPAQUE`; `None` if nothing is drawn.
pub fn text_rect(run: &Run, rect: Option<Rect>) -> Option<Rect> {
    let mut x = run.x;
    let mut y = run.y;
    match run.align & (TA_BOTTOM | TA_BASELINE) {
        TA_BASELINE => y = y.saturating_sub(run.ascent),
        TA_BOTTOM => y = y.saturating_sub(run.height),
        _ => {}
    }
    match run.align & (TA_RIGHT | TA_CENTER) {
        TA_CENTER => x = x.saturating_sub(run.width / 2),
        TA_RIGHT => x = x.saturating_sub(run.width),
        _ => {}
    }
    let mut text = Rect::new(
        x,
        y,
        x.saturating_add(run.width),
        y.saturating_add(run.height),
    );
    if let Some(rect) = rect {
        if run.options & ETO_CLIPPED != 0 {
            text = text.intersect(&rect);
        }
        if run.options & ETO_OPAQUE != 0 {
            text = text.union(&rect);
        }
    }
    (!text.is_empty()).then_some(text)
}

/// Where a `DrawText` call can draw: its rectangle, grown to the text's
/// calculated extent when it does not clip (`DT_NOCLIP`), within the DC's
/// clip box; `None` if nothing there is visible.
pub fn draw_text_area(rect: Rect, extent: Option<Rect>, clip: Option<Rect>) -> Option<Rect> {
    let area = extent.map_or(rect, |extent| rect.union(&extent));
    let area = clip.map_or(area, |clip| area.intersect(&clip));
    (!area.is_empty()).then_some(area)
}

/// Coverage (mask brightness) to alpha, for light or dark text.
pub struct AlphaTables {
    light: [u8; 256],
    dark: [u8; 256],
}

static TABLES: LazyLock<AlphaTables> = LazyLock::new(|| {
    let mut light = [0u8; 256];
    let mut dark = [0u8; 256];
    for i in 0..256 {
        // Inverse gamma from 1.2 for low to 1.5 for full coverage brightens
        // antialiased edges, as DrawTextWithGlow does.
        let a = i as f32 / 255.0;
        let gamma = 1.2 + 0.3 * a;
        light[i] = (a.powf(1.0 / gamma) * 255.0 + 0.5) as u8;
        // Dark text (light mode) goes over a light glass that shows the
        // wallpaper through, where thin strokes fade: fully opaque at full
        // coverage and edges boosted more than light text's (inverse
        // gamma 1.5 to 1.8).
        let gamma = 1.5 + 0.3 * a;
        dark[i] = (a.powf(1.0 / gamma) * 255.0 + 0.5) as u8;
    }
    AlphaTables { light, dark }
});

/// The coverage-to-alpha table for text of `color`.
pub fn alpha_table(color: ColorRef) -> &'static [u8; 256] {
    if is_dark(color) {
        &TABLES.dark
    } else {
        &TABLES.light
    }
}

fn is_dark(color: ColorRef) -> bool {
    let (r, g, b) = rgb(color);
    u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 < 128_000
}

/// Turns a white-on-black coverage mask of 32-bit BGRA pixels (rows of
/// `stride` pixels) into `color` with alpha, premultiplied. Black pixels,
/// where no text is, stay fully transparent. Rows or columns beyond the
/// slice are skipped.
pub fn colorize_mask(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    stride: usize,
    color: ColorRef,
) {
    let table = alpha_table(color);
    let (r, g, b) = rgb(color);
    let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
    let width = width.min(stride);
    for row in pixels.chunks_mut(stride.max(1)).take(height) {
        for px in row.iter_mut().take(width) {
            let (pb, pg, pr) = (*px & 0xff, (*px >> 8) & 0xff, (*px >> 16) & 0xff);
            if pb | pg | pr == 0 {
                continue;
            }
            let luma = ((pb + (pg << 1) + pr) >> 2) as usize;
            let alpha = u32::from(table[luma]);
            *px = (alpha << 24)
                | (((r * alpha) >> 8) << 16)
                | (((g * alpha) >> 8) << 8)
                | ((b * alpha) >> 8);
        }
    }
}

#[cfg(test)]
mod tests;
