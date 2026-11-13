//! Stops shell styling when Explorer keeps dying soon after sepiida
//! attaches to it.
//!
//! Explorer restarts by itself after a crash, and the agent attaches again
//! after every restart, so a DLL bug that crashes Explorer would otherwise
//! crash it in a loop. The guard is fed the time of each attach and each
//! Explorer restart as they happen; it needs no timer of its own.

use std::time::{Duration, Instant};

/// An Explorer restart this soon after an attach counts as a crash we may
/// have caused.
pub const CRASH_WINDOW: Duration = Duration::from_secs(30);
/// Crashes in a row after which styling is turned off.
pub const MAX_CRASHES: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Attach again.
    Continue,
    /// Turn shell styling off and tell the user.
    Disable,
}

#[derive(Debug, Clone)]
pub struct CrashGuard {
    window: Duration,
    max_crashes: u32,
    last_attach: Option<Instant>,
    crashes: u32,
}

impl Default for CrashGuard {
    fn default() -> Self {
        Self::new(CRASH_WINDOW, MAX_CRASHES)
    }
}

impl CrashGuard {
    pub fn new(window: Duration, max_crashes: u32) -> Self {
        Self {
            window,
            max_crashes: max_crashes.max(1),
            last_attach: None,
            crashes: 0,
        }
    }

    /// Crashes counted so far in the current run.
    pub fn crashes(&self) -> u32 {
        self.crashes
    }

    /// The DLL was attached at `now`. An earlier attach that outlived the
    /// window proves the DLL ran without crashing Explorer, so the count
    /// starts again.
    pub fn attached(&mut self, now: Instant) {
        if self
            .last_attach
            .is_some_and(|t| now.saturating_duration_since(t) > self.window)
        {
            self.crashes = 0;
        }
        self.last_attach = Some(now);
    }

    /// Explorer restarted at `now`. Our DLL went with the old process.
    pub fn explorer_restarted(&mut self, now: Instant) -> Verdict {
        match self.last_attach.take() {
            Some(t) if now.saturating_duration_since(t) <= self.window => self.crashes += 1,
            _ => self.crashes = 0,
        }
        if self.crashes >= self.max_crashes {
            Verdict::Disable
        } else {
            Verdict::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn restarts_right_after_attaching_disable_styling() {
        let start = Instant::now();
        let mut guard = CrashGuard::default();
        guard.attached(start);
        assert_eq!(
            guard.explorer_restarted(start + 5 * SECOND),
            Verdict::Continue
        );
        guard.attached(start + 7 * SECOND);
        assert_eq!(
            guard.explorer_restarted(start + 9 * SECOND),
            Verdict::Disable
        );
        assert_eq!(guard.crashes(), 2);
    }

    #[test]
    fn a_restart_long_after_attaching_is_not_a_crash() {
        let start = Instant::now();
        let mut guard = CrashGuard::default();
        guard.attached(start);
        assert_eq!(guard.explorer_restarted(start + SECOND), Verdict::Continue);
        guard.attached(start + 2 * SECOND);
        let later = start + 2 * SECOND + CRASH_WINDOW + SECOND;
        assert_eq!(guard.explorer_restarted(later), Verdict::Continue);
        assert_eq!(guard.crashes(), 0);
    }

    #[test]
    fn a_restart_without_an_attach_is_not_a_crash() {
        let start = Instant::now();
        let mut guard = CrashGuard::default();
        for n in 0..5 {
            assert_eq!(
                guard.explorer_restarted(start + n * SECOND),
                Verdict::Continue
            );
        }
        assert_eq!(guard.crashes(), 0);
    }

    #[test]
    fn an_attach_that_survived_the_window_clears_the_count() {
        let start = Instant::now();
        let mut guard = CrashGuard::default();
        guard.attached(start);
        guard.explorer_restarted(start + SECOND);
        guard.attached(start + 2 * SECOND);
        // Re-attached (e.g. a settings change) well after the last attach,
        // with no restart in between.
        guard.attached(start + 2 * SECOND + CRASH_WINDOW + SECOND);
        assert_eq!(guard.crashes(), 0);
    }

    #[test]
    fn at_least_one_crash_is_needed() {
        let start = Instant::now();
        let mut guard = CrashGuard::new(CRASH_WINDOW, 0);
        assert_eq!(guard.explorer_restarted(start), Verdict::Continue);
        guard.attached(start);
        assert_eq!(guard.explorer_restarted(start + SECOND), Verdict::Disable);
    }
}
