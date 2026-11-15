//! The RosePine theme by asev, from Windhawk's Windows 11 Taskbar Styler
//! mod by m417z (https://github.com/ramensoftware/windhawk-mods, GPL-3.0),
//! v1.11.1 (commit `2cd9c60`): no bar, each app button and the tray its
//! own rounded pill, a ring around the running app. Target and style lines
//! are copied unchanged, except:
//! - left out on purpose: hiding the Start button, the network icon and
//!   the notification centre button's icon (its choices, not ours),
//!   swapping two tray glyphs by their text, and the font (JetBrainsMono
//!   NF, not ours to ship);
//! - the hover thumbnails' fill, which WindowGlass's glass keeps
//!   (`window_glass`);
//! - colours: the pills (`#302d47`) are our glass in the palette's surface
//!   colour, the ring (`#ebbcba`) the accent, the label text (`#e0def4`)
//!   Windows' text colour.
//!
//! The ring is drawn per visual state (`@CommonStates`), which the engine
//! does not read yet; until then that entry is left out (and logged).

mod taskbar;

pub use taskbar::TASKBAR;
