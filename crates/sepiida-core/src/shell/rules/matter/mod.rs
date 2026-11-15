//! The Matter theme, from Windhawk's Windows 11 File Explorer Styler mod
//! by m417z (https://github.com/ramensoftware/windhawk-mods, GPL-3.0),
//! v1.7.2 (commit `12b5a5c`): File Explorer's look for the solid material
//! (the glass material uses WindowGlass's). Transparent bars, compact tabs
//! and an address bar and search box in the accent colour. Target and
//! style lines are copied unchanged, except that the lines hiding buttons
//! (back, forward, up, refresh, cut, copy, paste, rename, share, new, the
//! separator) and the secondary command bar are left out: they remove
//! function, not change the look.

mod explorer;

pub use explorer::EXPLORER;
