//! Whether the agent should attach the styling DLL right now: the setting,
//! the crash guard, and a short wait after an Explorer restart (the taskbar
//! window exists before its XAML does).
//!
//! All times are passed in. A restart carries the time it happened, not the
//! time the agent got round to it: the agent may have been busy attaching,
//! and judging by the later time made it skip every recheck (M4 test,
//! 2026-11-13).

use std::time::{Duration, Instant};

use super::crash_guard::{CrashGuard, Verdict};

/// No attach this soon after an Explorer restart.
pub const SETTLE_AFTER_RESTART: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Styling is off: take it down if present.
    Stop,
    /// Styling is on, but Explorer restarted moments ago: look again later.
    Wait,
    /// Attach if needed, then send colours.
    Go,
}

#[derive(Debug, Clone, Default)]
pub struct AttachGate {
    guard: CrashGuard,
    /// The setting as of the last decision.
    enabled: bool,
    /// Set by the crash guard; cleared once the setting is seen off, so
    /// turning it back on works.
    tripped: bool,
    not_before: Option<Instant>,
}

impl AttachGate {
    pub fn new(guard: CrashGuard) -> Self {
        Self {
            guard,
            ..Self::default()
        }
    }

    /// Whether styling is on and not stopped by the crash guard.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn crashes(&self) -> u32 {
        self.guard.crashes()
    }

    /// What to do after an apply at `now`, with the setting `setting`.
    pub fn decide(&mut self, setting: bool, now: Instant) -> Decision {
        if !setting {
            self.tripped = false;
            self.guard.reset();
        }
        self.enabled = setting && !self.tripped;
        if !self.enabled {
            return Decision::Stop;
        }
        match self.not_before {
            Some(due) if now < due => Decision::Wait,
            _ => {
                self.not_before = None;
                Decision::Go
            }
        }
    }

    /// An attach is being attempted at `now`.
    pub fn attaching(&mut self, now: Instant) {
        self.guard.attached(now);
    }

    /// Explorer restarted at `at`. Returns `Disable` once when the crash
    /// guard trips; styling then stays off until the setting is seen off.
    pub fn explorer_restarted(&mut self, at: Instant) -> Verdict {
        if !self.enabled {
            return Verdict::Continue;
        }
        self.not_before = Some(at + SETTLE_AFTER_RESTART);
        let verdict = self.guard.explorer_restarted(at);
        if verdict == Verdict::Disable {
            self.enabled = false;
            self.tripped = true;
        }
        verdict
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::crash_guard::CRASH_WINDOW;

    const SECOND: Duration = Duration::from_secs(1);

    fn enabled_gate(t0: Instant) -> AttachGate {
        let mut gate = AttachGate::default();
        assert_eq!(gate.decide(true, t0), Decision::Go);
        gate
    }

    #[test]
    fn the_setting_decides_between_stop_and_go() {
        let t0 = Instant::now();
        let mut gate = AttachGate::default();
        assert_eq!(gate.decide(false, t0), Decision::Stop);
        assert!(!gate.enabled());
        assert_eq!(gate.decide(true, t0), Decision::Go);
        assert!(gate.enabled());
    }

    #[test]
    fn after_a_restart_it_waits_then_goes() {
        let t0 = Instant::now();
        let mut gate = enabled_gate(t0);
        gate.explorer_restarted(t0 + 100 * SECOND);
        assert_eq!(gate.decide(true, t0 + 101 * SECOND), Decision::Wait);
        assert_eq!(gate.decide(true, t0 + 104 * SECOND), Decision::Go);
    }

    #[test]
    fn a_restart_handled_late_does_not_push_the_wait_back() {
        // Explorer restarted at t; the agent was busy and only handled it
        // 18 s later, together with the queued rechecks.
        let t0 = Instant::now();
        let mut gate = enabled_gate(t0);
        let restart = t0 + 100 * SECOND;
        gate.explorer_restarted(restart);
        assert_eq!(gate.decide(true, restart + 18 * SECOND), Decision::Go);
    }

    #[test]
    fn restarts_while_off_are_ignored() {
        let t0 = Instant::now();
        let mut gate = AttachGate::default();
        gate.decide(false, t0);
        assert_eq!(gate.explorer_restarted(t0), Verdict::Continue);
        assert_eq!(gate.decide(true, t0 + SECOND), Decision::Go);
    }

    #[test]
    fn the_crash_guard_stops_styling_until_the_setting_is_turned_off_and_on() {
        let t0 = Instant::now();
        let mut gate = enabled_gate(t0);
        gate.attaching(t0);
        assert_eq!(gate.explorer_restarted(t0 + 2 * SECOND), Verdict::Continue);
        gate.attaching(t0 + 6 * SECOND);
        assert_eq!(gate.explorer_restarted(t0 + 8 * SECOND), Verdict::Disable);
        assert_eq!(gate.crashes(), 2);
        // The settings file could not be saved: still on there, but no
        // attach this run.
        assert_eq!(gate.decide(true, t0 + 60 * SECOND), Decision::Stop);
        // Off, then on again: allowed.
        assert_eq!(gate.decide(false, t0 + 61 * SECOND), Decision::Stop);
        assert_eq!(gate.crashes(), 0);
        let later = t0 + 62 * SECOND + CRASH_WINDOW;
        assert_eq!(gate.decide(true, later), Decision::Go);
    }
}
