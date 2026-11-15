//! The glass on this thread's folder windows: the DWM
//! frame extended over the whole window and an accent blur tinted with
//! the palette (when to do what: `sepiida_core::shell::backdrop`).
//!
//! The calls and their order are the Windhawk File Explorer Styler's
//! (`ApplyBackgroundTranslucentEffect` with the blur effect, GPL-3.0). The
//! mod also hooks `DwmSetWindowAttribute` and `DwmExtendFrameIntoClientArea`
//! so Explorer cannot undo the glass; we have no function hooks yet, so a
//! refresh checks the backdrop type and applies again if it was reset.
//!
//! Only windows of the calling thread are touched: XAML threads are folder
//! window threads, and DWM attributes are set from the window's own
//! process.

use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    sync::OnceLock,
};

use sepiida_core::{
    Color,
    shell::{
        backdrop::{self, Action, Backdrop},
        framework::Framework,
    },
};
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, WPARAM},
        Graphics::{
            Dwm::{
                DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND, DWM_SYSTEMBACKDROP_TYPE,
                DWMSBT_AUTO, DWMSBT_TABBEDWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
                DWMWA_USE_HOSTBACKDROPBRUSH, DwmEnableBlurBehindWindow,
                DwmExtendFrameIntoClientArea, DwmGetWindowAttribute, DwmSetWindowAttribute,
            },
            Gdi::{
                CreateRectRgn, DeleteObject, HRGN, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME,
                RDW_INVALIDATE, RedrawWindow,
            },
        },
        System::{
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Threading::{GetCurrentProcessId, GetCurrentThreadId},
        },
        UI::{
            Controls::MARGINS,
            WindowsAndMessaging::{
                EnumThreadWindows, EnumWindows, FindWindowExW, GetClassNameW, GetPropW,
                GetWindowThreadProcessId, RemovePropW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SWP_NOZORDER, SendMessageW, SetPropW, WINDOWPOS, WM_DWMCOMPOSITIONCHANGED,
                WM_WINDOWPOSCHANGED,
            },
        },
    },
    core::{BOOL, HSTRING, PCWSTR, s, w},
};

use super::{gdi, hooks};
use crate::{host, log::log};

/// Window property marking a window that has our glass, so only those are
/// handed back.
pub(super) const APPLIED_PROP: PCWSTR = w!("sepiida.backdrop");

thread_local! {
    /// Windows handed back whose frame Explorer still has to recompute.
    /// Done by `flush`, outside the tree state's borrow: the messages make
    /// Explorer run code that may change the visual tree.
    static PENDING_UPDATES: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    /// Log lines already written once on this thread.
    static LOGGED_NO_WINDOW: Cell<bool> = const { Cell::new(false) };
    static LOGGED_READ_FAILED: Cell<bool> = const { Cell::new(false) };
}

/// Recomputes the frames of the windows handed back since the last call.
/// Call it after releasing the tree state.
pub fn flush() {
    let windows = PENDING_UPDATES.with(|pending| pending.take());
    if windows.is_empty() {
        return;
    }
    for window in windows {
        update_composition(window);
    }
    hooks::uninstall_if_unused();
    gdi::sync(process_has_glass);
}

/// Whether any top-level window of this process has our glass.
pub(super) fn process_has_glass() -> bool {
    let mut found = false;
    // SAFETY: the callback runs synchronously inside this call and gets a
    // pointer to `found`, which outlives it.
    let _ = unsafe { EnumWindows(Some(find_own_glass), LPARAM(&raw mut found as isize)) };
    found
}

unsafe extern "system" fn find_own_glass(window: HWND, found: LPARAM) -> BOOL {
    crate::guard("glass window search", BOOL(0), || {
        let mut pid = 0;
        // SAFETY: a window handle from the enumeration.
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        // SAFETY: plain query.
        if pid == unsafe { GetCurrentProcessId() } && has_glass(window) {
            // SAFETY: `found` is the `bool` passed by `process_has_glass`.
            unsafe { *(found.0 as *mut bool) = true };
            return BOOL(0); // stop
        }
        BOOL(1)
    })
}

/// Logs `line` the first time on this thread only.
fn log_once(flag: &'static std::thread::LocalKey<Cell<bool>>, line: impl FnOnce() -> String) {
    if !flag.with(|f| f.replace(true)) {
        log!("{}", line());
    }
}

/// One XAML thread's backdrop: the decision, plus whether this thread's
/// XAML is File Explorer's (the taskbar's thread gets none).
pub struct WindowBackdrop {
    state: Backdrop,
    enabled: bool,
}

impl WindowBackdrop {
    pub fn new(framework: Framework) -> Self {
        Self {
            state: Backdrop::default(),
            enabled: backdrop::applies(host::current(), framework),
        }
    }

    pub fn set_look(&mut self, on: bool) {
        let action = self.state.set_look(on);
        self.run(action, false);
    }

    pub fn set_tint(&mut self, tint: Color) {
        let action = self.state.set_tint(tint);
        self.run(action, false);
    }

    /// Applies again where Explorer reset the glass, or to a window that
    /// has none yet.
    pub fn refresh(&mut self) {
        let action = self.state.refresh();
        self.run(action, true);
        if self.enabled {
            // Removes the GDI hooks if one of them failed.
            // The glass stays: only a failed hook is acted on here.
            gdi::sync(|| true);
        }
    }

    /// `shell gdi on|off`: the switch is process-wide; each thread
    /// repaints its own glass windows.
    pub fn set_gdi(&mut self, on: bool) {
        if !self.enabled {
            return;
        }
        let windows: Vec<HWND> = glass_windows()
            .into_iter()
            .filter(|&w| has_glass(w))
            .collect();
        gdi::set_enabled(on, !windows.is_empty());
        for window in windows {
            redraw(window);
        }
    }

    pub fn restore(&mut self) {
        let action = self.state.restore();
        self.run(action, false);
    }

    fn run(&self, action: Option<Action>, only_if_reset: bool) {
        if !self.enabled {
            return;
        }
        let Some(action) = action else {
            return;
        };
        let windows = glass_windows();
        if windows.is_empty() {
            log_once(&LOGGED_NO_WINDOW, || {
                "backdrop: no folder window on this thread".into()
            });
        }
        for &window in &windows {
            match action {
                Action::Apply(tint) => {
                    if !only_if_reset || needs_glass(window) {
                        apply(window, tint);
                    }
                }
                Action::Remove => remove(window),
            }
        }
        gdi::set_thread_glass(windows.iter().any(|&w| has_glass(w)), false);
    }
}

/// This thread's top-level folder windows, without legacy frames.
fn glass_windows() -> Vec<HWND> {
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: the callback runs synchronously inside this call and gets a
    // pointer to `found`, which outlives it.
    let _ = unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect_glass_window),
            LPARAM(&raw mut found as isize),
        )
    };
    found
}

unsafe extern "system" fn collect_glass_window(window: HWND, found: LPARAM) -> BOOL {
    crate::guard("backdrop window enumeration", BOOL(0), || {
        if is_glass_window(window) {
            // SAFETY: `found` is the `Vec` passed by `glass_windows`.
            let found = unsafe { &mut *(found.0 as *mut Vec<HWND>) };
            found.push(window);
        }
        BOOL(1)
    })
}

fn is_glass_window(window: HWND) -> bool {
    let mut buffer = [0u16; 64];
    // SAFETY: a window handle from the enumeration; the buffer is ours.
    let length = unsafe { GetClassNameW(window, &mut buffer) };
    let Ok(length) = usize::try_from(length) else {
        return false;
    };
    let class = String::from_utf16_lossy(&buffer[..length.min(buffer.len())]);
    let legacy_child = HSTRING::from(backdrop::LEGACY_FRAME_CHILD_CLASS);
    // SAFETY: plain lookup of a child window by class.
    let has_legacy_child =
        unsafe { FindWindowExW(Some(window), None, &legacy_child, PCWSTR::null()) }.is_ok();
    backdrop::is_glass_window(&class, has_legacy_child)
}

/// Whether `window` lacks our glass: never given it, or Explorer set its
/// own backdrop type since.
fn needs_glass(window: HWND) -> bool {
    if !has_glass(window) {
        return true;
    }
    let mut kind = DWM_SYSTEMBACKDROP_TYPE::default();
    // SAFETY: `kind` is a DWM_SYSTEMBACKDROP_TYPE, the attribute's type.
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&raw mut kind).cast(),
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        )
    };
    match read {
        Ok(()) if kind == DWMSBT_AUTO => false,
        Ok(()) => {
            log!("backdrop: {window:?} was reset (backdrop type {})", kind.0);
            true
        }
        Err(error) => {
            // Cannot tell; leave the glass as it is.
            log_once(&LOGGED_READ_FAILED, || {
                format!("backdrop: cannot read the backdrop type: {error}")
            });
            false
        }
    }
}

fn apply(window: HWND, tint: Color) {
    log!("backdrop: glass on {window:?}, tint {tint}");
    // Hooks are patched at a window's first glass only: a new window may
    // come with modules loaded since; a colour update does not.
    let first = !has_glass(window);
    // The mark and the hooks first, as in the mod: Explorer's own calls in
    // response to ours then already keep the glass.
    // SAFETY: our process's window; the value is a flag, not a pointer.
    if let Err(error) = unsafe {
        SetPropW(
            window,
            APPLIED_PROP,
            Some(HANDLE(std::ptr::without_provenance_mut(1))),
        )
    } {
        log!("backdrop: SetPropW failed: {error}");
    }
    // Before the redraw below, so the content repaints with alpha; the
    // colours again, as the theme may have changed.
    gdi::set_thread_glass(true, true);
    if first {
        hooks::install();
        gdi::install();
    }
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: our process's window; `margins` lives across the call.
    if let Err(error) = unsafe { DwmExtendFrameIntoClientArea(window, &margins) } {
        log!("backdrop: DwmExtendFrameIntoClientArea failed: {error}");
    }
    set_backdrop_type(window, DWMSBT_AUTO);
    set_blur_behind(window, true);
    set_accent_blur(window, tint);
    // No composition update, unlike the mod after a settings change: the
    // laptop test showed the glass without one, and it would make Explorer
    // run its frame code at once.
    redraw(window);
}

fn remove(window: HWND) {
    if !has_glass(window) {
        return;
    }
    log!("backdrop: glass off {window:?}");
    set_backdrop_type(window, DWMSBT_TABBEDWINDOW);
    set_blur_behind(window, false);
    // WinUI sets this when it creates the window (ACCENT_ENABLE_HOSTBACKDROP);
    // setting it again replaces our accent blur.
    let host_backdrop = BOOL(1);
    // SAFETY: `host_backdrop` is a BOOL, the attribute's type.
    if let Err(error) = unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_HOSTBACKDROPBRUSH,
            (&raw const host_backdrop).cast(),
            size_of::<BOOL>() as u32,
        )
    } {
        log!("backdrop: DWMWA_USE_HOSTBACKDROPBRUSH failed: {error}");
    }
    // SAFETY: removing our own property.
    let _ = unsafe { RemovePropW(window, APPLIED_PROP) };
    // Explorer extends its frame again with its own margins (in `flush`).
    PENDING_UPDATES.with(|pending| pending.borrow_mut().push(window));
}

/// Whether `window` has our glass. Safe from any thread.
pub(super) fn has_glass(window: HWND) -> bool {
    // SAFETY: reading a property by name.
    !unsafe { GetPropW(window, APPLIED_PROP) }.is_invalid()
}

fn set_backdrop_type(window: HWND, kind: DWM_SYSTEMBACKDROP_TYPE) {
    // SAFETY: `kind` is a DWM_SYSTEMBACKDROP_TYPE, the attribute's type.
    if let Err(error) = unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&raw const kind).cast(),
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        )
    } {
        log!(
            "backdrop: DWMWA_SYSTEMBACKDROP_TYPE {} failed: {error}",
            kind.0
        );
    }
}

/// Blur behind over the whole window; without it DWM draws the extended
/// frame over the accent blur.
fn set_blur_behind(window: HWND, on: bool) {
    // SAFETY: an empty-rectangle region, deleted below.
    let region = if on {
        unsafe { CreateRectRgn(0, 0, -1, -1) }
    } else {
        HRGN::default()
    };
    let blur = DWM_BLURBEHIND {
        dwFlags: DWM_BB_ENABLE | if on { DWM_BB_BLURREGION } else { 0 },
        fEnable: BOOL::from(on),
        hRgnBlur: region,
        fTransitionOnMaximized: BOOL(0),
    };
    // SAFETY: `blur` lives across the call; DWM copies the region.
    if let Err(error) = unsafe { DwmEnableBlurBehindWindow(window, &blur) } {
        log!("backdrop: DwmEnableBlurBehindWindow failed: {error}");
    }
    if !region.is_invalid() {
        // SAFETY: the region was created above and is not used after this.
        let _ = unsafe { DeleteObject(region.into()) };
    }
}

/// `ACCENT_POLICY`, undocumented, as user32 reads it.
#[repr(C)]
struct AccentPolicy {
    accent_state: i32,
    accent_flags: i32,
    gradient_color: u32,
    animation_id: i32,
}

/// `WINDOWCOMPOSITIONATTRIBDATA`, undocumented.
#[repr(C)]
struct CompositionAttribute {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
const WCA_ACCENT_POLICY: u32 = 19;

type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(HWND, *mut CompositionAttribute) -> BOOL;

/// user32's `SetWindowCompositionAttribute`, if this Windows exports it.
fn set_window_composition_attribute() -> Option<SetWindowCompositionAttributeFn> {
    static FUNCTION: OnceLock<Option<SetWindowCompositionAttributeFn>> = OnceLock::new();
    *FUNCTION.get_or_init(|| {
        // SAFETY: user32 is loaded in every process with windows.
        let user32 = unsafe { GetModuleHandleW(w!("user32.dll")) }.ok()?;
        // SAFETY: looking up an export by name.
        let address = unsafe { GetProcAddress(user32, s!("SetWindowCompositionAttribute")) };
        if address.is_none() {
            log!("backdrop: user32 has no SetWindowCompositionAttribute");
        }
        // SAFETY: the export has had this signature since Windows 7.
        address.map(|f| unsafe {
            std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                SetWindowCompositionAttributeFn,
            >(f)
        })
    })
}

fn set_accent_blur(window: HWND, tint: Color) {
    let Some(set) = set_window_composition_attribute() else {
        return;
    };
    let mut policy = AccentPolicy {
        accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
        accent_flags: 0,
        gradient_color: backdrop::gradient_color(tint),
        animation_id: 0,
    };
    let mut attribute = CompositionAttribute {
        attribute: WCA_ACCENT_POLICY,
        data: (&raw mut policy).cast(),
        size: size_of::<AccentPolicy>(),
    };
    // SAFETY: both structs live across the call and match user32's layout.
    if !unsafe { set(window, &raw mut attribute) }.as_bool() {
        log!("backdrop: SetWindowCompositionAttribute failed");
    }
}

/// Makes the window recompute its frame, as after a DWM change.
fn update_composition(window: HWND) {
    let position = WINDOWPOS {
        hwnd: window,
        flags: SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        ..Default::default()
    };
    // SAFETY: our thread's window; `position` lives across the call.
    unsafe {
        SendMessageW(
            window,
            WM_WINDOWPOSCHANGED,
            Some(WPARAM(0)),
            Some(LPARAM(&raw const position as isize)),
        );
        SendMessageW(
            window,
            WM_DWMCOMPOSITIONCHANGED,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
    redraw(window);
}

/// Repaints the content, which draws differently over the glass.
fn redraw(window: HWND) {
    // SAFETY: our thread's window.
    let _ = unsafe {
        RedrawWindow(
            Some(window),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN,
        )
    };
}

#[cfg(all(test, windows))]
mod tests;
