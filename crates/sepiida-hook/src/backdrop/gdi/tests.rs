//! The hook bodies called directly on 32-bit DIB sections: exact
//! pass-through on a thread without glass, alpha on a glass window's DC.
//! Whether Explorer's own calls reach the hooks is only seen on Windows.

use std::sync::Mutex;

use windows::{
    Win32::{
        Foundation::{COLORREF, HANDLE, HWND, RECT},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, COLOR_WINDOW, CreateCompatibleDC,
            CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject,
            ETO_CLIPPED, ExtTextOutW, GdiFlush, GetSysColor, HDC, HGDIOBJ, SelectObject, SetBkMode,
            SetTextColor, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, SetPropW,
            WINDOW_EX_STYLE, WNDCLASSEXW, WS_CHILD, WS_OVERLAPPEDWINDOW,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

use super::{
    draw_text::draw_text as draw_text_hook,
    fill::fill_rect,
    install::hook_guard,
    parts::paint_part,
    state::{
        self, glass_window_of, pop_painting, push_painting, set_thread_glass, with_uxtheme_text,
    },
    text::ext_text_out,
};
use crate::backdrop::glass::APPLIED_PROP;

/// The switch and the failure flag are process-wide.
static LOCK: Mutex<()> = Mutex::new(());

const SIZE: i32 = 64;

/// A memory DC with a top-down 32-bit DIB section, all pixels zero.
struct Canvas {
    dc: HDC,
    bitmap: HGDIOBJ,
    bits: *mut u32,
}

impl Canvas {
    fn new() -> Self {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: SIZE,
                biHeight: -SIZE,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: a fresh DC and DIB section, freed in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            let bitmap = CreateDIBSection(
                Some(dc),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                Some(HANDLE::default()),
                0,
            )
            .unwrap();
            SelectObject(dc, bitmap.into());
            SetBkMode(dc, TRANSPARENT);
            Self {
                dc,
                bitmap: bitmap.into(),
                bits: bits.cast(),
            }
        }
    }

    fn pixels(&self) -> Vec<u32> {
        // SAFETY: GDI is done drawing after the flush; SIZE² pixels.
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.bits, (SIZE * SIZE) as usize).to_vec()
        }
    }

    fn fill_all(&self, value: u32) {
        // SAFETY: as in `pixels`.
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts_mut(self.bits, (SIZE * SIZE) as usize).fill(value);
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: made in `new`, not used after this.
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bitmap);
        }
    }
}

extern "system" fn default_proc(
    window: HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: forwarding the arguments we received.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// A hidden top-level window marked with our glass, and a child of
/// `child_class` (registered here unless it is a system class).
struct GlassWindow {
    top: HWND,
    child: HWND,
}

impl GlassWindow {
    fn new(child_class: PCWSTR) -> Self {
        // SAFETY: the test executable's own handle.
        let instance = unsafe { GetModuleHandleW(PCWSTR::null()) }.unwrap().into();
        let class = HSTRING::from("sepiida-gdi-test");
        let info = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(default_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        // SAFETY: fully initialised; registering twice fails harmlessly.
        unsafe { RegisterClassExW(&info) };
        let child_class = if child_class.is_null() {
            PCWSTR(class.as_ptr())
        } else {
            child_class
        };
        // SAFETY: registered classes; hidden windows destroyed in Drop.
        unsafe {
            let top = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                &class,
                &class,
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                200,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let child = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                child_class,
                PCWSTR::null(),
                WS_CHILD,
                0,
                0,
                100,
                100,
                Some(top),
                None,
                Some(instance),
                None,
            )
            .unwrap();
            SetPropW(
                top,
                APPLIED_PROP,
                Some(HANDLE(std::ptr::without_provenance_mut(1))),
            )
            .unwrap();
            Self { top, child }
        }
    }
}

impl Drop for GlassWindow {
    fn drop(&mut self) {
        // SAFETY: created on this thread by the test.
        let _ = unsafe { DestroyWindow(self.top) };
    }
}

/// Marks this thread as having glass and `window` as being painted for
/// the duration of `body`.
fn painting<T>(window: HWND, body: impl FnOnce() -> T) -> T {
    set_thread_glass(true, false);
    assert!(push_painting(window));
    let result = body();
    pop_painting(window);
    set_thread_glass(false, false);
    result
}

fn draw_text(dc: HDC, hooked: bool) {
    let text: Vec<u16> = "Hamburgefonstiv".encode_utf16().collect();
    let rect = RECT {
        left: 0,
        top: 0,
        right: SIZE,
        bottom: SIZE,
    };
    // SAFETY: our canvas; the text and rectangle live across the call.
    unsafe {
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        if hooked {
            let _ = ext_text_out(
                dc,
                2,
                2,
                ETO_CLIPPED.0,
                &rect,
                PCWSTR(text.as_ptr()),
                text.len() as u32,
                std::ptr::null(),
            );
        } else {
            let _ = ExtTextOutW(
                dc,
                2,
                2,
                ETO_CLIPPED,
                Some(&rect),
                PCWSTR(text.as_ptr()),
                text.len() as u32,
                None,
            );
        }
    }
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn text_on_a_thread_without_glass_is_drawn_exactly_as_gdi_draws_it() {
    let _lock = lock();
    let (plain, hooked) = (Canvas::new(), Canvas::new());
    draw_text(plain.dc, false);
    draw_text(hooked.dc, true);
    let pixels = plain.pixels();
    assert!(pixels.iter().any(|&p| p != 0), "the font drew nothing");
    assert!(
        pixels.iter().all(|&p| p >> 24 == 0),
        "GDI leaves alpha at zero"
    );
    assert_eq!(hooked.pixels(), pixels);
}

#[test]
fn text_on_a_glass_window_gets_alpha() {
    let _lock = lock();
    let window = GlassWindow::new(PCWSTR::null());
    let canvas = Canvas::new();
    painting(window.child, || {
        assert_eq!(glass_window_of(canvas.dc), Some(window.child));
        draw_text(canvas.dc, true);
    });
    let pixels = canvas.pixels();
    let drawn: Vec<u32> = pixels.iter().copied().filter(|&p| p != 0).collect();
    assert!(!drawn.is_empty(), "nothing drawn");
    assert!(
        drawn.iter().all(|&p| p >> 24 > 0),
        "every glyph pixel has alpha"
    );
    // Premultiplied: no colour byte above the alpha.
    assert!(
        drawn
            .iter()
            .all(|&p| (0..3).all(|i| (p >> (i * 8)) & 0xff <= p >> 24))
    );
}

#[test]
fn text_is_left_alone_while_uxtheme_composites_it() {
    let _lock = lock();
    let window = GlassWindow::new(PCWSTR::null());
    let canvas = Canvas::new();
    painting(window.child, || {
        with_uxtheme_text(|| {
            assert_eq!(glass_window_of(canvas.dc), None);
            draw_text(canvas.dc, true);
        });
        assert!(glass_window_of(canvas.dc).is_some(), "the flag is put back");
    });
    assert!(canvas.pixels().iter().all(|&p| p >> 24 == 0));
}

fn fill(dc: HDC, color: u32) {
    let rect = RECT {
        left: 0,
        top: 0,
        right: SIZE,
        bottom: SIZE,
    };
    // SAFETY: our canvas; a brush made and freed here.
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        fill_rect(dc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

#[test]
fn background_fills_on_a_glass_window_become_transparent_black() {
    let _lock = lock();
    // SAFETY: plain query.
    let window_color = unsafe { GetSysColor(COLOR_WINDOW) };
    let window = GlassWindow::new(PCWSTR::null());
    let canvas = Canvas::new();
    canvas.fill_all(0x1122_3344);
    painting(window.child, || fill(canvas.dc, window_color));
    assert!(canvas.pixels().iter().all(|&p| p == 0));
    // Any other colour is filled as GDI fills it.
    let other = if window_color == 0xff { 0xff00 } else { 0xff };
    painting(window.child, || fill(canvas.dc, other));
    let expected = if other == 0xff {
        0x00ff_0000
    } else {
        0x0000_ff00
    };
    assert!(canvas.pixels().iter().all(|&p| p == expected));
    // Without glass on the thread, the window colour is filled.
    fill(canvas.dc, window_color);
    assert!(
        canvas
            .pixels()
            .iter()
            .all(|&p| p & 0x00ff_ffff != 0 || window_color == 0)
    );
}

#[test]
fn edit_control_fills_are_made_opaque() {
    let _lock = lock();
    let window = GlassWindow::new(w!("Edit"));
    let canvas = Canvas::new();
    painting(window.child, || fill(canvas.dc, 0x0000_00ff));
    assert!(
        canvas.pixels().iter().all(|&p| p == 0xffff_0000),
        "{:#x}",
        canvas.pixels()[0]
    );
}

#[test]
fn a_dc_of_a_window_without_glass_is_not_ours() {
    let _lock = lock();
    let window = GlassWindow::new(PCWSTR::null());
    // SAFETY: removing the mark set by GlassWindow.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::RemovePropW(window.top, APPLIED_PROP).unwrap();
    }
    let canvas = Canvas::new();
    painting(window.child, || {
        assert_eq!(glass_window_of(canvas.dc), None)
    });
}

#[test]
fn painting_pops_nested_windows_whose_end_was_missed() {
    let _lock = lock();
    let outer = GlassWindow::new(PCWSTR::null());
    let inner = GlassWindow::new(PCWSTR::null());
    let canvas = Canvas::new();
    set_thread_glass(true, false);
    assert!(push_painting(outer.child));
    assert!(push_painting(inner.child));
    assert_eq!(glass_window_of(canvas.dc), Some(inner.child));
    pop_painting(outer.child);
    assert_eq!(glass_window_of(canvas.dc), None, "both popped");
    set_thread_glass(false, false);
    assert!(!push_painting(outer.child), "nothing tracked without glass");
}

#[test]
fn a_panicking_hook_turns_every_hook_into_a_pass_through() {
    let _lock = lock();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = hook_guard("test hook", || -> i32 { panic!("boom") });
    std::panic::set_hook(previous);
    assert_eq!(result, None);
    assert!(state::failed());
    let window = GlassWindow::new(PCWSTR::null());
    set_thread_glass(true, false);
    assert!(!push_painting(window.child));
    set_thread_glass(false, false);
    state::reset_after_test();
    assert_eq!(hook_guard("test hook", || 7), Some(7));
}

fn draw_text_on(dc: HDC, format: u32) -> (i32, RECT) {
    let text: Vec<u16> = "Hamburgefonstiv".encode_utf16().collect();
    let mut rect = RECT {
        left: 2,
        top: 2,
        right: SIZE - 2,
        bottom: SIZE - 2,
    };
    // SAFETY: our canvas; the text and rectangle live across the call.
    let height = unsafe {
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        draw_text_hook(
            dc,
            PCWSTR(text.as_ptr()),
            text.len() as i32,
            &mut rect,
            format,
        )
    };
    (height, rect)
}

#[test]
fn draw_text_without_glass_is_gdis_and_with_glass_gets_alpha() {
    let _lock = lock();
    let plain = Canvas::new();
    let (height, _) = draw_text_on(plain.dc, 0);
    assert!(height > 0);
    let pixels = plain.pixels();
    assert!(pixels.iter().any(|&p| p != 0), "the font drew nothing");
    assert!(pixels.iter().all(|&p| p >> 24 == 0));

    let window = GlassWindow::new(PCWSTR::null());
    let glass = Canvas::new();
    let (glass_height, _) = painting(window.child, || draw_text_on(glass.dc, 0));
    assert_eq!(glass_height, height, "the real call's result comes back");
    let drawn: Vec<u32> = glass.pixels().into_iter().filter(|&p| p != 0).collect();
    assert!(!drawn.is_empty(), "nothing drawn");
    assert!(
        drawn.iter().all(|&p| p >> 24 > 0),
        "every glyph pixel has alpha"
    );
    // Where GDI drew, the glass copy drew too.
    let covered = |px: &[u32]| px.iter().map(|&p| p != 0).collect::<Vec<_>>();
    assert_eq!(covered(&glass.pixels()), covered(&pixels));
}

#[test]
fn draw_text_measuring_draws_nothing_and_measures_as_gdi() {
    let _lock = lock();
    // DT_CALCRECT.
    let calc = 0x400;
    let plain = Canvas::new();
    let expected = draw_text_on(plain.dc, calc);
    let window = GlassWindow::new(PCWSTR::null());
    let glass = Canvas::new();
    let measured = painting(window.child, || draw_text_on(glass.dc, calc));
    assert_eq!(measured.0, expected.0);
    assert_eq!(
        (measured.1.right, measured.1.bottom),
        (expected.1.right, expected.1.bottom)
    );
    assert!(glass.pixels().iter().all(|&p| p == 0));
}

fn full() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: SIZE,
        bottom: SIZE,
    }
}

#[test]
fn cleared_parts_show_the_glass_and_thumbs_are_translucent() {
    use sepiida_core::shell::gdi_glass::parts::{ArrowState, Direction, Paint};
    let _lock = lock();
    let canvas = Canvas::new();
    canvas.fill_all(0xffff_ffff);
    assert!(paint_part(
        canvas.dc,
        96,
        Paint::Clear,
        &full(),
        std::ptr::null()
    ));
    assert!(
        canvas.pixels().iter().all(|&p| p == 0),
        "a track is transparent"
    );

    canvas.fill_all(0xffff_ffff);
    let thumb = Paint::Thumb {
        horizontal: false,
        hot: false,
    };
    assert!(paint_part(canvas.dc, 96, thumb, &full(), std::ptr::null()));
    let pixels = canvas.pixels();
    let row = &pixels[(SIZE / 2 * SIZE) as usize..((SIZE / 2 + 1) * SIZE) as usize];
    assert_eq!(row[0], 0, "clear beside the bar");
    assert_eq!(
        row[(SIZE / 2) as usize] >> 24,
        128,
        "the bar at the theme's alpha"
    );
    // A second time comes from the cache, drawn the same.
    canvas.fill_all(0);
    assert!(paint_part(canvas.dc, 96, thumb, &full(), std::ptr::null()));
    assert_eq!(canvas.pixels(), pixels);

    // A hidden arrow only clears; a hovered one draws.
    canvas.fill_all(0xffff_ffff);
    let hidden = Paint::Arrow(Direction::Up, ArrowState::Normal);
    assert!(paint_part(canvas.dc, 96, hidden, &full(), std::ptr::null()));
    assert!(canvas.pixels().iter().all(|&p| p == 0));
    let hot = Paint::Arrow(Direction::Up, ArrowState::Hot);
    assert!(paint_part(canvas.dc, 96, hot, &full(), std::ptr::null()));
    assert!(canvas.pixels().iter().any(|&p| p >> 24 > 0));
}

#[test]
fn a_part_is_drawn_only_inside_its_clip() {
    use sepiida_core::shell::gdi_glass::parts::Paint;
    let _lock = lock();
    let canvas = Canvas::new();
    canvas.fill_all(0xffff_ffff);
    let clip = RECT {
        left: 0,
        top: 0,
        right: SIZE / 2,
        bottom: SIZE,
    };
    assert!(paint_part(canvas.dc, 96, Paint::Clear, &full(), &clip));
    let pixels = canvas.pixels();
    assert_eq!(pixels[0], 0);
    assert_eq!(pixels[(SIZE - 1) as usize], 0xffff_ffff, "outside the clip");
}

#[test]
fn a_progress_track_is_a_translucent_bar() {
    use sepiida_core::shell::gdi_glass::parts::Paint;
    let _lock = lock();
    let canvas = Canvas::new();
    canvas.fill_all(0xffff_ffff);
    assert!(paint_part(
        canvas.dc,
        96,
        Paint::Track,
        &full(),
        std::ptr::null()
    ));
    let pixels = canvas.pixels();
    assert_eq!(pixels[(SIZE / 2 * SIZE + SIZE / 2) as usize] >> 24, 56);
    assert!(pixels[0] >> 24 < 56, "rounded corner");
}
