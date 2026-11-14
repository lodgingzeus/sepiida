//! Which shell process the DLL is loaded in, and so which rules apply.
//! Decided from our own executable's file name; a process that is not a
//! known shell host gets no rules and is never styled.

use std::sync::OnceLock;

use sepiida_core::shell::{
    hosts::Host,
    rules::{Rule, rules_for},
};

use crate::log::log;

/// The host this process is, if it is one.
pub fn current() -> Option<Host> {
    #[cfg(all(test, windows))]
    if let Some(host) = *tests::OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) {
        return Some(host);
    }
    static HOST: OnceLock<Option<Host>> = OnceLock::new();
    *HOST.get_or_init(|| {
        let exe = std::env::current_exe().ok();
        let host = exe
            .as_deref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .and_then(Host::from_exe);
        if host.is_none() {
            log!("{exe:?} is not a shell host sepiida styles; no rules apply");
        }
        host
    })
}

/// The rules for this process; none outside the known hosts.
pub fn rules() -> &'static [Rule] {
    current().map_or(&[], rules_for)
}

#[cfg(all(test, windows))]
pub mod tests {
    use std::sync::Mutex;

    use sepiida_core::shell::hosts::Host;

    /// The host the DLL's tests pretend to run in: the test runner is not
    /// a shell process.
    pub static OVERRIDE: Mutex<Option<Host>> = Mutex::new(None);

    pub fn pretend(host: Host) {
        *OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = Some(host);
    }
}
