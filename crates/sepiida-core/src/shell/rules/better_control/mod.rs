//! The BetterControl11 theme by TheGamer1445891, from Windhawk's Windows 11
//! Notification Center Styler mod by m417z
//! (https://github.com/ramensoftware/windhawk-mods, GPL-3.0), v1.8
//! (commit `4197cf6`): quick settings as a grid of large tiles with the
//! brightness and volume sliders standing upright in a card beside them,
//! and the notification centre, calendar and media card as joined, thinly
//! outlined panels. Target and style lines are copied unchanged, except
//! one empty `Shadow:=` (it would leave out the whole media card entry);
//! its two overlay variables point at resources of its own, replaced by
//! ones Windows has (`flyouts`).
//! The theme sets no panel backgrounds; ours come after it (`materials`):
//! our glass on the panels, solid raised cards. Its own inner cards use a
//! 3.5 % white veil and Windows' overlay resources, kept as written.
//!
//! Windows setting it expects: Date & time > "Show time in Notification
//! Center". Its README also asks to arrange the quick actions as in its
//! screenshots (some move offscreen otherwise).

mod flyouts;
mod materials;

pub use flyouts::FLYOUTS;
pub use materials::MATERIALS;
