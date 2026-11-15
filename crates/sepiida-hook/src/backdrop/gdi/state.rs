//! What the GDI hooks know: whether the hooks may act (process-wide), and
//! per thread whether it has a glass window, which windows it is
//! painting, and which memory DCs double-buffer them.
//!
//! The hooks run for every GDI call in explorer.exe, taskbar included, so
//! the first check is a thread-local flag, and nothing here allocates or
//! has a destructor.

use std::{
    cell::Cell,
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

use sepiida_core::shell::gdi_glass::{self, DARK_BACKGROUND_PARTS};
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::Gdi::{COLOR_WINDOW, GetSysColor, HDC, WindowFromDC},
        UI::{
            Controls::{CloseThemeData, GetThemeColor, OpenThemeData, TMT_FILLCOLOR},
            WindowsAndMessaging::{GA_ROOT, GetAncestor},
        },
    },
    core::HSTRING,
};

use super::super::glass::has_glass;
use crate::log::log;

/// Painting nested deeper, or more double buffers at once, than this are
/// attributed to the outer window or not tracked, as in the mod.
const DEPTH: usize = 8;

/// A window a DC draws to, and whether it belongs to a glass window.
#[derive(Clone, Copy, Default)]
pub(super) struct DcWindow {
    window: usize,
    glass: bool,
}

impl DcWindow {
    fn new(window: HWND) -> Self {
        Self {
            window: window.0 as usize,
            glass: is_glass_part(window),
        }
    }

    fn hwnd(self) -> HWND {
        HWND(std::ptr::without_provenance_mut::<c_void>(self.window))
    }
}

#[derive(Clone, Copy)]
struct Painting {
    windows: [DcWindow; DEPTH],
    len: usize,
}

#[derive(Clone, Copy)]
struct Buffers {
    dcs: [(usize, DcWindow); DEPTH],
    len: usize,
}

#[derive(Clone, Copy)]
struct Fills {
    colors: [u32; DARK_BACKGROUND_PARTS.len()],
    len: usize,
}

const NO_WINDOW: DcWindow = DcWindow {
    window: 0,
    glass: false,
};

thread_local! {
    static GLASS_THREAD: Cell<bool> = const { Cell::new(false) };
    /// Windows between BeginPaint and EndPaint, innermost last. Pushed
    /// before BeginPaint, which sends WM_ERASEBKGND.
    static PAINTING: Cell<Painting> = const {
        Cell::new(Painting { windows: [NO_WINDOW; DEPTH], len: 0 })
    };
    /// comctl32's double-buffer memory DCs and their window; also drawn to
    /// outside painting (the scroll bar fade).
    static BUFFERS: Cell<Buffers> = const {
        Cell::new(Buffers { dcs: [(0, NO_WINDOW); DEPTH], len: 0 })
    };
    /// Set while uxtheme composites text itself: it draws a white mask
    /// with GDI and derives alpha from it, which must not be altered.
    static UXTHEME_TEXT: Cell<bool> = const { Cell::new(false) };
    /// The content background's fill colours in dark mode.
    static DARK_FILLS: Cell<Fills> = const {
        Cell::new(Fills { colors: [0; DARK_BACKGROUND_PARTS.len()], len: 0 })
    };
}

/// `shell gdi on|off`.
static ENABLED: AtomicBool = AtomicBool::new(true);
/// A hook panicked: everything passes through from now on.
static FAILED: AtomicBool = AtomicBool::new(false);

/// Whether the hooks may act in this process.
pub(super) fn active() -> bool {
    ENABLED.load(Ordering::Relaxed) && !FAILED.load(Ordering::Relaxed)
}

pub(super) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub(super) fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

/// Lets the hooks act again after a test made one fail.
#[cfg(all(test, windows))]
pub(super) fn reset_after_test() {
    FAILED.store(false, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
}

/// Stops every hook from acting; returns whether this is the first time.
pub(super) fn fail() -> bool {
    !FAILED.swap(true, Ordering::Relaxed)
}

/// Whether the hooks act on this thread. False during thread teardown.
fn thread_active() -> bool {
    active() && GLASS_THREAD.try_with(Cell::get).unwrap_or(false)
}

/// Marks whether this thread has a window with our glass; called where
/// the glass goes on and off. Loads the dark-mode fill colours when it
/// turns on, or with `reload` (the theme may have changed).
pub fn set_thread_glass(on: bool, reload: bool) {
    let was = GLASS_THREAD.try_with(|g| g.replace(on)).unwrap_or(false);
    if on {
        if !was || reload {
            load_dark_fills();
        }
    } else {
        // Nothing painted from now on is ours; forget what was tracked.
        let _ = PAINTING.try_with(|p| {
            p.set(Painting {
                windows: [NO_WINDOW; DEPTH],
                len: 0,
            })
        });
        let _ = BUFFERS.try_with(|b| {
            b.set(Buffers {
                dcs: [(0, NO_WINDOW); DEPTH],
                len: 0,
            })
        });
    }
}

fn load_dark_fills() {
    let mut fills = Fills {
        colors: [0; DARK_BACKGROUND_PARTS.len()],
        len: 0,
    };
    for (class, part) in DARK_BACKGROUND_PARTS {
        // SAFETY: opens theme data by class name, closed below.
        let theme = unsafe { OpenThemeData(None, &HSTRING::from(class)) };
        if theme.is_invalid() {
            continue;
        }
        // SAFETY: a theme handle opened above.
        if let Ok(color) = unsafe { GetThemeColor(theme, part, 0, TMT_FILLCOLOR) } {
            fills.colors[fills.len] = color.0;
            fills.len += 1;
        }
        // SAFETY: as above; not used after this.
        let _ = unsafe { CloseThemeData(theme) };
    }
    log!("gdi: {} dark-mode background colour(s) known", fills.len);
    let _ = DARK_FILLS.try_with(|f| f.set(fills));
}

/// Whether a fill of `color` paints the content background.
pub(super) fn is_background(color: u32) -> bool {
    let fills = DARK_FILLS.try_with(Cell::get).unwrap_or(Fills {
        colors: [0; DARK_BACKGROUND_PARTS.len()],
        len: 0,
    });
    // SAFETY: plain query.
    let window = unsafe { GetSysColor(COLOR_WINDOW) };
    gdi_glass::is_window_background(color, window, &fills.colors[..fills.len])
}

/// Whether `window` is part of a top-level window with our glass.
fn is_glass_part(window: HWND) -> bool {
    // SAFETY: plain query; a stale handle yields null.
    let root = unsafe { GetAncestor(window, GA_ROOT) };
    !root.is_invalid() && has_glass(root)
}

/// Before BeginPaint: remembers the window being painted. Returns whether
/// it was pushed.
pub(super) fn push_painting(window: HWND) -> bool {
    if !thread_active() {
        return false;
    }
    let entry = DcWindow::new(window);
    PAINTING
        .try_with(|cell| {
            let mut painting = cell.get();
            if painting.len >= DEPTH {
                return false;
            }
            painting.windows[painting.len] = entry;
            painting.len += 1;
            cell.set(painting);
            true
        })
        .unwrap_or(false)
}

/// At EndPaint (or a failed BeginPaint): forgets the window, and any
/// painted inside it whose EndPaint was missed.
pub(super) fn pop_painting(window: HWND) {
    let _ = PAINTING.try_with(|cell| {
        let mut painting = cell.get();
        if painting.len == 0 {
            return;
        }
        let key = window.0 as usize;
        if let Some(at) = painting.windows[..painting.len]
            .iter()
            .rposition(|w| w.window == key)
        {
            painting.len = at;
            cell.set(painting);
        }
    });
}

/// A memory DC comctl32 created compatible with `target`'s DC. Returns
/// whether it double-buffers a glass window.
pub(super) fn track_buffer(memory: HDC, target: HDC) -> bool {
    if !thread_active() || memory.is_invalid() || target.is_invalid() {
        return false;
    }
    // SAFETY: plain query on a DC the caller passed.
    let window = unsafe { WindowFromDC(target) };
    if window.is_invalid() {
        return false;
    }
    let entry = DcWindow::new(window);
    BUFFERS
        .try_with(|cell| {
            let mut buffers = cell.get();
            if buffers.len >= DEPTH {
                return false;
            }
            buffers.dcs[buffers.len] = (memory.0 as usize, entry);
            buffers.len += 1;
            cell.set(buffers);
            entry.glass
        })
        .unwrap_or(false)
}

pub(super) fn forget_buffer(dc: HDC) {
    let _ = BUFFERS.try_with(|cell| {
        let mut buffers = cell.get();
        let key = dc.0 as usize;
        if let Some(at) = buffers.dcs[..buffers.len]
            .iter()
            .position(|(d, _)| *d == key)
        {
            buffers.len -= 1;
            buffers.dcs[at] = buffers.dcs[buffers.len];
            cell.set(buffers);
        }
    });
}

/// The window of `dc` if what is drawn on it must carry alpha: a DC of a
/// glass window's part, or a memory DC attributed to one (its double
/// buffer's window, else the window being painted). `None` for everything
/// else, and while uxtheme composites text.
pub(super) fn glass_window_of(dc: HDC) -> Option<HWND> {
    if !thread_active() || UXTHEME_TEXT.try_with(Cell::get).unwrap_or(true) {
        return None;
    }
    // SAFETY: plain query on a DC the caller passed.
    let window = unsafe { WindowFromDC(dc) };
    if !window.is_invalid() {
        return is_glass_part(window).then_some(window);
    }
    let key = dc.0 as usize;
    let buffered = BUFFERS
        .try_with(|cell| {
            let buffers = cell.get();
            buffers.dcs[..buffers.len]
                .iter()
                .find(|(d, _)| *d == key)
                .map(|(_, w)| *w)
        })
        .ok()
        .flatten();
    let painting = || {
        PAINTING
            .try_with(|cell| {
                let painting = cell.get();
                painting.len.checked_sub(1).map(|top| painting.windows[top])
            })
            .ok()
            .flatten()
    };
    buffered
        .or_else(painting)
        .filter(|w| w.glass)
        .map(DcWindow::hwnd)
}

/// Runs `body` with the hooks leaving text alone (uxtheme composites it).
pub(super) fn with_uxtheme_text<T>(body: impl FnOnce() -> T) -> T {
    /// Puts the flag back even if `body` panics.
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            let _ = UXTHEME_TEXT.try_with(|t| t.set(self.0));
        }
    }
    let _reset = Reset(UXTHEME_TEXT.try_with(|t| t.replace(true)).unwrap_or(false));
    body()
}
