//! Which calls are hooked, and when: patched with the glass, put back once
//! no window of the process has it, when switched off, or after a hook
//! panicked.

use std::sync::atomic::{AtomicBool, Ordering};

use sepiida_core::pe::Import;

use super::{draw_text, fill, paint, parts, state, text};
use crate::{
    guard,
    imports::{self, HookSet, Replacement},
    log::log,
};

/// user32's painting calls; `-ntuser-` matches its API sets.
const USER32: HookSet = HookSet {
    name: "gdi user32",
    dlls: &["user32", "-ntuser-"],
    implementers: &["user32"],
    only_in: None,
};
/// GDI's calls; `-gdi-` matches its API sets, `gdi32` also gdi32full.
const GDI: HookSet = HookSet {
    name: "gdi gdi32",
    dlls: &["gdi32", "-gdi-"],
    implementers: &["gdi32"],
    only_in: None,
};
/// comctl32 creates a memory DC for each double-buffered paint; uxtheme
/// reuses its own for other targets, so only comctl32's are tracked.
const GDI_COMCTL32: HookSet = HookSet {
    name: "gdi comctl32",
    only_in: Some("comctl32.dll"),
    ..GDI
};
/// The navigation pane divider, a line ExplorerFrame draws.
const GDI_EXPLORER_FRAME: HookSet = HookSet {
    name: "gdi explorerframe",
    only_in: Some("explorerframe.dll"),
    ..GDI
};
const UXTHEME: HookSet = HookSet {
    name: "gdi uxtheme",
    dlls: &["uxtheme"],
    implementers: &["uxtheme"],
    only_in: None,
};
/// DirectUI (the file list, its column header, the status bar) calls
/// uxtheme's theme-part drawing through pointers it stored, not imports.
const UXTHEME_DUI_STORED: HookSet = HookSet {
    name: "gdi uxtheme stored (dui70)",
    only_in: Some("dui70.dll"),
    ..UXTHEME
};
const SETS: [&HookSet; 6] = [
    &USER32,
    &GDI,
    &GDI_COMCTL32,
    &GDI_EXPLORER_FRAME,
    &UXTHEME,
    &UXTHEME_DUI_STORED,
];
/// uxtheme's `DrawTextWithGlow`, exported by ordinal only.
pub(super) const DRAW_TEXT_WITH_GLOW: u16 = 126;

fn replacement(function: Import<'static>, address: usize) -> Replacement {
    Replacement { function, address }
}

/// Hooks the calls in every module loaded now (again after modules
/// loaded; slots already ours are skipped). Nothing while switched off or
/// after a panic.
pub fn install() {
    if !state::active() {
        return;
    }
    let mut count = 0;
    count += imports::patch(
        &USER32,
        &[
            replacement(
                Import::Name("BeginPaint"),
                paint::begin_paint as *const () as usize,
            ),
            replacement(
                Import::Name("EndPaint"),
                paint::end_paint as *const () as usize,
            ),
            replacement(
                Import::Name("FillRect"),
                fill::fill_rect as *const () as usize,
            ),
            replacement(
                Import::Name("DrawTextW"),
                draw_text::draw_text as *const () as usize,
            ),
            replacement(
                Import::Name("DrawTextExW"),
                draw_text::draw_text_ex_hook as *const () as usize,
            ),
        ],
    );
    count += imports::patch(
        &GDI,
        &[
            replacement(Import::Name("PatBlt"), fill::pat_blt as *const () as usize),
            replacement(
                Import::Name("ExtTextOutW"),
                text::ext_text_out as *const () as usize,
            ),
            replacement(
                Import::Name("DeleteDC"),
                paint::delete_dc as *const () as usize,
            ),
        ],
    );
    count += imports::patch(
        &GDI_COMCTL32,
        &[replacement(
            Import::Name("CreateCompatibleDC"),
            paint::create_compatible_dc as *const () as usize,
        )],
    );
    count += imports::patch(
        &GDI_EXPLORER_FRAME,
        &[replacement(
            Import::Name("Polyline"),
            fill::polyline as *const () as usize,
        )],
    );
    count += imports::patch(
        &UXTHEME,
        &[
            replacement(
                Import::Name("DrawThemeTextEx"),
                text::draw_theme_text_ex as *const () as usize,
            ),
            replacement(
                Import::Name("DrawThemeBackground"),
                parts::draw_background as *const () as usize,
            ),
            replacement(
                Import::Name("DrawThemeBackgroundEx"),
                parts::draw_background_ex as *const () as usize,
            ),
            replacement(
                Import::Ordinal(DRAW_TEXT_WITH_GLOW),
                text::draw_text_with_glow as *const () as usize,
            ),
        ],
    );
    count += imports::patch_stored(&UXTHEME_DUI_STORED, &parts::stored_functions());
    if count > 0 {
        log!("gdi hooks: {count} import slot(s) hooked");
    }
}

fn uninstall(why: &str) {
    let count: usize = SETS.iter().map(|set| imports::unpatch(set.name)).sum();
    if count > 0 {
        log!("gdi hooks: {count} import slot(s) put back ({why})");
    }
}

fn installed() -> bool {
    SETS.iter().any(|set| imports::is_patched(set.name))
}

/// Puts the hooks back after a panic, or once `process_has_glass` says
/// no window has the glass (asked only then). Call it from a
/// control-window thread, not from a hook.
pub fn sync(process_has_glass: impl FnOnce() -> bool) {
    if !installed() {
        return;
    }
    if state::failed() {
        uninstall("a hook failed");
    } else if !process_has_glass() {
        uninstall("no glass left");
    }
}

/// `shell gdi on|off`, for the whole process. On hooks again if this
/// thread has glass; the caller repaints its windows.
pub fn set_enabled(on: bool, thread_has_glass: bool) {
    state::set_enabled(on);
    log!("gdi: switched {}", if on { "on" } else { "off" });
    if !on {
        uninstall("switched off");
    } else if state::failed() {
        log!("gdi: stays off for this Explorer: a hook failed earlier");
    } else if thread_has_glass {
        install();
    }
}

/// Runs a hook's work; a panic turns every hook into a pass-through
/// (`None`, the caller then makes the real call).
pub(super) fn hook_guard<T>(context: &str, body: impl FnOnce() -> T) -> Option<T> {
    let mut done = None;
    guard(context, (), || done = Some(body()));
    if done.is_none() && state::fail() {
        log!("gdi: hooks pass everything through from now on ({context} failed)");
    }
    done
}

/// Hook kinds whose first call on a glass window is logged, to see in the
/// test log which hooks Explorer's drawing reaches.
#[derive(Clone, Copy)]
pub(super) enum Seen {
    BeginPaint,
    FillRect,
    PatBlt,
    TextOut,
    TextOutAlpha,
    DrawText,
    DrawTextAlpha,
    UxthemeText,
    GlowText,
    Divider,
    ThemePart,
    Buffer,
}

const KINDS: usize = Seen::Buffer as usize + 1;
static SEEN: [AtomicBool; KINDS] = [const { AtomicBool::new(false) }; KINDS];

pub(super) fn seen(kind: Seen) {
    if !SEEN[kind as usize].swap(true, Ordering::Relaxed) {
        let name = match kind {
            Seen::BeginPaint => "BeginPaint",
            Seen::FillRect => "FillRect",
            Seen::PatBlt => "PatBlt",
            Seen::TextOut => "ExtTextOutW",
            Seen::TextOutAlpha => "ExtTextOutW drawn with alpha",
            Seen::DrawText => "DrawText",
            Seen::DrawTextAlpha => "DrawText drawn with alpha",
            Seen::UxthemeText => "DrawThemeTextEx (composited)",
            Seen::GlowText => "DrawTextWithGlow",
            Seen::Divider => "Polyline (divider)",
            Seen::ThemePart => "theme part (scroll bar, header or separator)",
            Seen::Buffer => "CreateCompatibleDC (comctl32)",
        };
        log!("gdi hook: first {name} on a glass window");
    }
}
