//! Which window a DC belongs to: BeginPaint/EndPaint mark the window being
//! painted, comctl32's CreateCompatibleDC/DeleteDC its double buffers.
//! Every call goes on unchanged.

use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{BeginPaint, CreateCompatibleDC, DeleteDC, EndPaint, HDC, PAINTSTRUCT},
};
use windows_core::BOOL;

use super::{
    install::{Seen, hook_guard, seen},
    state,
};

pub(super) unsafe extern "system" fn begin_paint(window: HWND, paint: *mut PAINTSTRUCT) -> HDC {
    let pushed = hook_guard("BeginPaint hook", || state::push_painting(window)).unwrap_or(false);
    // SAFETY: the caller's arguments, passed on unchanged.
    let dc = unsafe { BeginPaint(window, paint) };
    if pushed {
        hook_guard("BeginPaint hook", || {
            if dc.is_invalid() {
                state::pop_painting(window);
            } else {
                seen(Seen::BeginPaint);
            }
        });
    }
    dc
}

pub(super) unsafe extern "system" fn end_paint(window: HWND, paint: *const PAINTSTRUCT) -> BOOL {
    hook_guard("EndPaint hook", || state::pop_painting(window));
    // SAFETY: the caller's arguments, passed on unchanged.
    unsafe { EndPaint(window, paint) }
}

/// Patched in comctl32 only.
pub(super) unsafe extern "system" fn create_compatible_dc(dc: HDC) -> HDC {
    let target = (!dc.is_invalid()).then_some(dc);
    // SAFETY: the caller's argument, passed on unchanged.
    let memory = unsafe { CreateCompatibleDC(target) };
    hook_guard("CreateCompatibleDC hook", || {
        if state::track_buffer(memory, dc) {
            seen(Seen::Buffer);
        }
    });
    memory
}

pub(super) unsafe extern "system" fn delete_dc(dc: HDC) -> BOOL {
    hook_guard("DeleteDC hook", || state::forget_buffer(dc));
    // SAFETY: the caller's argument, passed on unchanged.
    unsafe { DeleteDC(dc) }
}
