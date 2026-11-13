//! Notices when the system light/dark mode ("Windows mode", which the
//! taskbar follows) changes, so shell colours made for the old mode are
//! replaced at once.
//!
//! Broadcasts proved unreliable in M2 (the wallpaper one never reached the
//! agent's window), so this waits on the registry key itself with
//! `RegNotifyChangeKeyValue`. The key holds other values too; only a change
//! of the mode is reported.

use std::thread;

use sepiida_core::accent::{self, ThemeMode};
use tracing::warn;
use windows::{
    Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_QUERY_VALUE, REG_NOTIFY_CHANGE_LAST_SET,
            RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW,
        },
    },
    core::HSTRING,
};

use super::theme;

#[derive(Debug, thiserror::Error)]
pub enum ModeWatchError {
    #[error("could not open HKCU\\{0} to watch the light/dark mode")]
    Open(&'static str, #[source] windows::core::Error),
    #[error("could not start the light/dark mode watcher")]
    Thread(#[source] std::io::Error),
}

/// Calls `on_change` with the new mode whenever the system mode changes.
/// The thread waits in the kernel between changes and runs until the
/// process exits.
pub fn spawn(on_change: impl Fn(ThemeMode) + Send + 'static) -> Result<(), ModeWatchError> {
    let key_name = accent::system_mode_value().key;
    let key = Key::open(key_name)?;
    thread::Builder::new()
        .name("mode-watch".to_owned())
        .spawn(move || watch(&key, &on_change))
        .map_err(ModeWatchError::Thread)?;
    Ok(())
}

fn watch(key: &Key, on_change: &dyn Fn(ThemeMode)) {
    let mut last = theme::system_mode();
    loop {
        // SAFETY: a key opened with KEY_NOTIFY; synchronous, so the call
        // returns after the next change to one of the key's values.
        let status = unsafe {
            RegNotifyChangeKeyValue(key.0, false, REG_NOTIFY_CHANGE_LAST_SET, None, false)
        };
        if status != ERROR_SUCCESS {
            warn!(
                "the light/dark mode watcher stopped: {}",
                windows::core::Error::from(status.to_hresult())
            );
            return;
        }
        let now = theme::system_mode();
        if now != last {
            last = now;
            on_change(now);
        }
    }
}

/// An open registry key, closed on drop.
struct Key(HKEY);

// SAFETY: a registry key handle may be used from any thread.
unsafe impl Send for Key {}

impl Key {
    fn open(name: &'static str) -> Result<Self, ModeWatchError> {
        let mut key = HKEY::default();
        // SAFETY: `key` is writable; the name outlives the call.
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(name),
                None,
                KEY_NOTIFY | KEY_QUERY_VALUE,
                &mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(ModeWatchError::Open(
                name,
                windows::core::Error::from(status.to_hresult()),
            ));
        }
        Ok(Self(key))
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: opened once in `open`, closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
