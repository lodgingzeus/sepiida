//! Native theming: write the accent colour and light/dark mode, then tell
//! running programs to pick the change up.

use sepiida_core::{accent::RegistryWrite, snapshot::RestoreAction};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        },
    },
    core::w,
};

use super::registry::{self, RegistryError};

/// How long each top-level window gets to handle the broadcast. A hung
/// window must not stall sepiida.
const BROADCAST_TIMEOUT_MS: u32 = 200;

/// Returns the writes that are not already in the registry. Every write
/// makes all running programs repaint, so unchanged values are left alone.
pub fn pending(writes: Vec<RegistryWrite>) -> Result<Vec<RegistryWrite>, RegistryError> {
    let mut pending = Vec::new();
    for write in writes {
        if registry::read(&write)?.as_ref() != Some(&write.data) {
            pending.push(write);
        }
    }
    Ok(pending)
}

/// Writes the values and broadcasts the change once. Does nothing for an
/// empty list.
pub fn apply(writes: &[RegistryWrite]) -> Result<(), RegistryError> {
    if writes.is_empty() {
        return Ok(());
    }
    // If a write fails part-way, still announce the ones that succeeded so
    // the shell does not show a mix of old and new until the next change.
    let result = writes.iter().try_for_each(registry::set);
    broadcast_color_change();
    result
}

/// Puts saved values back and broadcasts the change once. Every action is
/// attempted even if one fails; the first error is returned.
pub fn restore(actions: &[RestoreAction]) -> Result<(), RegistryError> {
    if actions.is_empty() {
        return Ok(());
    }
    let mut first_error = None;
    for action in actions {
        let result = match action {
            RestoreAction::Set(write) => registry::set(write),
            RestoreAction::Delete { key, name } => registry::delete(key, name),
        };
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    }
    broadcast_color_change();
    first_error.map_or(Ok(()), Err)
}

fn broadcast_color_change() {
    // SAFETY: the string is a static, null-terminated literal, and the call
    // waits for each window (up to the timeout) before returning. A failure
    // only means some window did not answer in time, which is not an error
    // for us.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            BROADCAST_TIMEOUT_MS,
            None,
        );
    }
}
