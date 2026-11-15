//! Theme parts over the glass, as the Windhawk File Explorer Styler draws
//! them (GPL-3.0; sizes and colours as written there): scroll bar tracks,
//! the column header background and the pane separator become clear,
//! and a progress bar's track (the drive capacity bars) a soft translucent
//! bar under uxtheme's own fill (ours, not the mod's); the
//! scroll thumb becomes a slim rounded bar, its arrows show only when
//! hovered, and a hot or pressed column header gets a rounded shade.
//!
//! Shapes are drawn here into premultiplied 32-bit BGRA pixels with 4×4
//! supersampling, for the DLL to alpha-blend (the mod uses Direct2D).

use super::text::Rect;

// Scroll bar parts and states (vsstyle.h).
const SBP_ARROWBTN: i32 = 1;
const SBP_THUMBBTNHORZ: i32 = 2;
const SBP_THUMBBTNVERT: i32 = 3;
const SBP_LOWERTRACKHORZ: i32 = 4;
const SBP_UPPERTRACKVERT: i32 = 7;
const SCRBS_NORMAL: i32 = 1;
const ABS_UPNORMAL: i32 = 1;
const ABS_RIGHTDISABLED: i32 = 16;
const ABS_UPHOVER: i32 = 17;
const ABS_RIGHTHOVER: i32 = 20;
const HP_HEADERITEM: i32 = 1;
// Progress bar tracks (vsstyle.h): PP_BAR, PP_BARVERT, PP_TRANSPARENTBAR,
// PP_TRANSPARENTBARVERT.
const PROGRESS_TRACKS: [i32; 4] = [1, 2, 11, 12];

/// An arrow's direction, as the arrow states order them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// An arrow's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowState {
    /// Hidden until the scroll bar is hovered.
    Normal,
    Hot,
    Pressed,
    Disabled,
    /// The scroll bar is hovered, the arrow is not.
    BarHovered,
}

/// How a theme part is painted over the glass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Cleared to transparent black (the glass shows).
    Clear,
    Thumb {
        horizontal: bool,
        hot: bool,
    },
    Arrow(Direction, ArrowState),
    /// A hot or pressed column header item, over a clear background.
    HeaderItem {
        hot: bool,
    },
    /// A progress bar's track: a translucent rounded bar.
    Track,
}

/// How the part of theme class `class` (ASCII case ignored) is painted,
/// or `None` to leave it to uxtheme.
pub fn paint_for(class: &str, part: i32, state: i32) -> Option<Paint> {
    if class.eq_ignore_ascii_case("ScrollBar") {
        return match part {
            SBP_ARROWBTN => arrow(state).map(|(d, s)| Paint::Arrow(d, s)),
            SBP_THUMBBTNHORZ | SBP_THUMBBTNVERT => Some(Paint::Thumb {
                horizontal: part == SBP_THUMBBTNHORZ,
                hot: state != SCRBS_NORMAL,
            }),
            SBP_LOWERTRACKHORZ..=SBP_UPPERTRACKVERT => Some(Paint::Clear),
            _ => None,
        };
    }
    if class.eq_ignore_ascii_case("Header") {
        // Item states cycle through normal, hot and pressed.
        return match part {
            0 => Some(Paint::Clear),
            HP_HEADERITEM if state % 3 == 1 => Some(Paint::Clear),
            HP_HEADERITEM => Some(Paint::HeaderItem {
                hot: state % 3 == 2,
            }),
            _ => None,
        };
    }
    if class.eq_ignore_ascii_case("Progress") && PROGRESS_TRACKS.contains(&part) {
        return Some(Paint::Track);
    }
    // The separator between the navigation pane and the content.
    if class.eq_ignore_ascii_case("PreviewPane") && matches!(part, 3 | 4) {
        return Some(Paint::Clear);
    }
    None
}

fn arrow(state: i32) -> Option<(Direction, ArrowState)> {
    const DIRECTIONS: [Direction; 4] = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];
    match state {
        ABS_UPNORMAL..=ABS_RIGHTDISABLED => {
            let index = state - ABS_UPNORMAL;
            let look = match index % 4 {
                0 => ArrowState::Normal,
                1 => ArrowState::Hot,
                2 => ArrowState::Pressed,
                _ => ArrowState::Disabled,
            };
            Some((DIRECTIONS[(index / 4) as usize], look))
        }
        ABS_UPHOVER..=ABS_RIGHTHOVER => Some((
            DIRECTIONS[(state - ABS_UPHOVER) as usize],
            ArrowState::BarHovered,
        )),
        _ => None,
    }
}

/// A colour with alpha, not premultiplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argb(pub u8, pub u8, pub u8, pub u8);

/// Premultiplied BGRA pixels, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

/// A bitmap and the margins kept unscaled when it is stretched (left,
/// top, right, bottom), in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NineGrid {
    pub bitmap: Bitmap,
    pub margins: [i32; 4],
}

const SAMPLES: usize = 4;

/// Draws `inside` (a point test in pixel coordinates) in `color`, with
/// the coverage of each pixel taken from `SAMPLES`² sample points.
fn raster(width: usize, height: usize, color: Argb, inside: impl Fn(f32, f32) -> bool) -> Bitmap {
    let mut pixels = vec![0u32; width * height];
    let step = 1.0 / SAMPLES as f32;
    for y in 0..height {
        for x in 0..width {
            let mut hits = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) * step;
                    let py = y as f32 + (sy as f32 + 0.5) * step;
                    hits += u32::from(inside(px, py));
                }
            }
            if hits > 0 {
                let alpha = u32::from(color.0) * hits / (SAMPLES * SAMPLES) as u32;
                let scale = |c: u8| u32::from(c) * alpha / 255;
                pixels[y * width + x] =
                    (alpha << 24) | (scale(color.1) << 16) | (scale(color.2) << 8) | scale(color.3);
            }
        }
    }
    Bitmap {
        width,
        height,
        pixels,
    }
}

/// Whether `(x, y)` is inside the rectangle `l, t, r, b` with corners of
/// `radius`; `round_top` false keeps the top corners square.
fn in_rounded(
    x: f32,
    y: f32,
    (l, t, r, b): (f32, f32, f32, f32),
    radius: f32,
    round_top: bool,
) -> bool {
    if x < l || x > r || y < t || y > b {
        return false;
    }
    let radius = radius.min((r - l) / 2.0).min((b - t) / 2.0).max(0.0);
    let cx = if x < l + radius {
        l + radius
    } else if x > r - radius {
        r - radius
    } else {
        return true;
    };
    let cy = if round_top && y < t + radius {
        t + radius
    } else if y > b - radius {
        b - radius
    } else {
        return true;
    };
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

fn scaled(value: f32, scale: f32) -> usize {
    (value * scale).max(1.0) as usize
}

/// The scroll thumb: wider when hovered.
pub fn thumb(horizontal: bool, hot: bool, dpi: u32) -> NineGrid {
    let scale = dpi.max(1) as f32 / 96.0;
    let width = scaled(if horizontal { 20.0 } else { 17.0 }, scale);
    let height = scaled(if horizontal { 17.0 } else { 11.0 }, scale);
    let (w, h) = (width as f32, height as f32);
    let inset = if hot { 0.25 } else { 0.35 };
    let bar = if horizontal {
        (0.0, h * inset, w, h - h * inset)
    } else {
        (w * inset, 0.0, w - w * inset, h)
    };
    let color = if hot {
        Argb(160, 224, 224, 224)
    } else {
        Argb(128, 160, 160, 160)
    };
    let radius = 4.0 * scale;
    let bitmap = raster(width, height, color, |x, y| {
        in_rounded(x, y, bar, radius, true)
    });
    let m = |v: f32| (v * scale) as i32;
    NineGrid {
        bitmap,
        margins: [m(8.0), m(5.0), m(8.0), m(5.0)],
    }
}

/// A hot or pressed column header: a shade with rounded bottom corners.
pub fn header_item(hot: bool, dpi: u32) -> NineGrid {
    let scale = dpi.max(1) as f32 / 96.0;
    let size = scaled(24.0, scale);
    let color = if hot {
        Argb(96, 144, 144, 144)
    } else {
        Argb(64, 144, 144, 144)
    };
    let s = size as f32;
    let bitmap = raster(size, size, color, |x, y| {
        in_rounded(x, y, (0.0, 0.0, s, s), 6.0 * scale, false)
    });
    let m = |v: f32| (v * scale) as i32;
    NineGrid {
        bitmap,
        margins: [m(12.0), 0, m(11.0), m(12.0)],
    }
}

/// A progress bar's track: neutral grey at low alpha, so it reads on
/// light and dark glass alike, with slightly rounded corners.
pub fn track(dpi: u32) -> NineGrid {
    let scale = dpi.max(1) as f32 / 96.0;
    let size = scaled(12.0, scale);
    let s = size as f32;
    let bitmap = raster(size, size, Argb(56, 128, 128, 128), |x, y| {
        in_rounded(x, y, (0.0, 0.0, s, s), 2.0 * scale, true)
    });
    let m = (3.0 * scale) as i32;
    NineGrid {
        bitmap,
        margins: [m; 4],
    }
}

/// A scroll arrow filling `width` × `height`; `None` for a hidden one.
pub fn arrow_bitmap(
    direction: Direction,
    state: ArrowState,
    width: usize,
    height: usize,
    dpi: u32,
) -> Option<Bitmap> {
    let scale = dpi.max(1) as f32 / 96.0;
    let (mut base, mut tall, mut color) = (7.0 * scale, 4.5 * scale, Argb(128, 160, 160, 160));
    match state {
        ArrowState::Normal => return None,
        ArrowState::Hot => {
            base = 8.0 * scale;
            tall = 5.5 * scale;
            color = Argb(192, 224, 224, 224);
        }
        ArrowState::Disabled => color = Argb(192, 64, 64, 64),
        ArrowState::Pressed | ArrowState::BarHovered => {}
    }
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    // An up arrow with a thick base, turned clockwise for the others.
    let points = [
        (cx - 1.0 - base / 2.0, cy + tall / 2.0),
        (cx - 1.0 - base / 2.0, cy + 2.0 + tall / 2.0),
        (cx + base / 2.0, cy + 2.0 + tall / 2.0),
        (cx + base / 2.0, cy + tall / 2.0),
        (cx, cy - tall / 2.0),
        (cx - 1.0, cy - tall / 2.0),
    ];
    let degrees: f32 = match direction {
        Direction::Up => 0.0,
        Direction::Down => 180.0,
        Direction::Left => 270.0,
        Direction::Right => 90.0,
    };
    let (sin, cos) = degrees.to_radians().sin_cos();
    Some(raster(width, height, color, |x, y| {
        // The sample turned back into the up arrow's frame.
        let (dx, dy) = (x - cx, y - cy);
        let ux = cx + dx * cos + dy * sin;
        let uy = cy - dx * sin + dy * cos;
        in_polygon(ux, uy, &points)
    }))
}

/// Even-odd point-in-polygon test.
fn in_polygon(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The nine pieces that stretch a `width` × `height` bitmap over `dst`
/// keeping `margins` unscaled: (destination, source) pairs, empty pieces
/// left out. Margins larger than `dst` are cut down as the mod does.
pub fn nine_grid(dst: Rect, width: i32, height: i32, margins: [i32; 4]) -> Vec<(Rect, Rect)> {
    let (dw, dh) = (dst.width(), dst.height());
    let left = margins[0].clamp(0, dw);
    let right = margins[2].clamp(0, (dw - left).max(0));
    let top = margins[1].clamp(0, dh);
    let bottom = margins[3].clamp(0, (dh - top).max(0));
    let src_x = [0, left, width - right, width];
    let src_y = [0, top, height - bottom, height];
    let dst_x = [dst.left, dst.left + left, dst.right - right, dst.right];
    let dst_y = [dst.top, dst.top + top, dst.bottom - bottom, dst.bottom];
    let mut pieces = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            let d = Rect::new(dst_x[i], dst_y[j], dst_x[i + 1], dst_y[j + 1]);
            let s = Rect::new(src_x[i], src_y[j], src_x[i + 1], src_y[j + 1]);
            if !d.is_empty() && !s.is_empty() {
                pieces.push((d, s));
            }
        }
    }
    pieces
}

#[cfg(test)]
mod tests;
