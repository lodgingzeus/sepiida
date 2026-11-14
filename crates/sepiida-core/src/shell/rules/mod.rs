//! What the hook DLL styles, as data: which elements (type, `x:Name` and a
//! required ancestor), which property, and what it gets: a palette colour,
//! or a layout value while the look is on. New surfaces are added as rules
//! here rather than as code in the DLL. Each shell process has its own rule
//! set (`rules_for`): hand-written colour rules and the taskbar's floating
//! bar, with Windhawk theme lines on top (`windhawk`, `window_glass`).

mod by_host;
mod flyouts;
mod rule;
mod sandboxed;
mod start;
mod taskbar;
pub mod windhawk;
mod window_glass;

pub use by_host::{rules_for, skipped_for};
pub use rule::{
    BORDER_TYPE, Effect, GRID_TYPE, Name, Paint, Part, RECTANGLE_TYPE, Role, Rule, RuleId, Step,
};
pub use start::START_RULES;
pub use taskbar::{TASKBAR_BACKGROUND, TASKBAR_FRAME_NAME, TASKBAR_RULES, taskbar_bar_margin};
