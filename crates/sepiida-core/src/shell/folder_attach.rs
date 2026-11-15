//! When the agent attaches to File Explorer's XAML: once per explorer.exe
//! that has a folder window, one attempt at a time, a few retries when
//! the XAML is not up yet (it starts with the first folder window), then
//! no more until the next apply.
//!
//! An attached copy is never detached and attached again by the agent: a
//! second WinUI attach in one process has ended Explorer. Turning styling
//! off restores it instead, and a later start just sends the colours.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

/// Waits before trying again after a failed attempt, in order.
pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];

/// What to do for a process with a folder window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Attached: send the colours and look.
    Send,
    /// Start an attach (now marked as running).
    Attach,
    /// An attach is running, or the retries are used up.
    Wait,
}

#[derive(Debug, Default)]
pub struct FolderAttaches {
    running: HashSet<u32>,
    failures: HashMap<u32, usize>,
}

impl FolderAttaches {
    /// Decides for `pid`, given whether the DLL is attached there now.
    pub fn next(&mut self, pid: u32, attached: bool) -> Next {
        if attached {
            self.failures.remove(&pid);
            return Next::Send;
        }
        if self.running.contains(&pid)
            || self.failures.get(&pid).copied().unwrap_or(0) > RETRY_DELAYS.len()
        {
            return Next::Wait;
        }
        self.running.insert(pid);
        Next::Attach
    }

    /// An attach to `pid` ended. Returns how long to wait before trying
    /// again, if it failed and retries are left.
    pub fn done(&mut self, pid: u32, attached: bool) -> Option<Duration> {
        self.running.remove(&pid);
        if attached {
            self.failures.remove(&pid);
            return None;
        }
        let failures = self.failures.entry(pid).or_default();
        *failures += 1;
        RETRY_DELAYS.get(*failures - 1).copied()
    }

    /// A new apply: processes whose retries ran out may be tried again.
    /// Forgets processes no longer running (pids are reused).
    pub fn new_round(&mut self, running: &[u32]) {
        self.failures.retain(|pid, _| running.contains(pid));
        for failures in self.failures.values_mut() {
            *failures = (*failures).min(RETRY_DELAYS.len());
        }
    }

    pub fn clear(&mut self) {
        self.failures.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attached_process_gets_the_colours() {
        let mut a = FolderAttaches::default();
        assert_eq!(a.next(7, true), Next::Send);
    }

    #[test]
    fn one_attach_at_a_time_per_process() {
        let mut a = FolderAttaches::default();
        assert_eq!(a.next(7, false), Next::Attach);
        assert_eq!(a.next(7, false), Next::Wait);
        assert_eq!(a.next(8, false), Next::Attach);
        assert_eq!(a.done(7, true), None);
        assert_eq!(a.next(7, true), Next::Send);
    }

    #[test]
    fn failures_retry_with_growing_waits_then_stop() {
        let mut a = FolderAttaches::default();
        let mut waits = Vec::new();
        while a.next(7, false) == Next::Attach {
            match a.done(7, false) {
                Some(wait) => waits.push(wait),
                None => {
                    // Out of retries: nothing more until a new round.
                    assert_eq!(a.next(7, false), Next::Wait);
                    break;
                }
            }
        }
        assert_eq!(waits, RETRY_DELAYS);
    }

    #[test]
    fn a_new_round_allows_one_more_try_and_forgets_ended_processes() {
        let mut a = FolderAttaches::default();
        for _ in 0..=RETRY_DELAYS.len() {
            assert_eq!(a.next(7, false), Next::Attach);
            a.done(7, false);
        }
        assert_eq!(a.next(7, false), Next::Wait);
        a.new_round(&[7]);
        assert_eq!(a.next(7, false), Next::Attach);
        assert_eq!(
            a.done(7, false),
            None,
            "one try per round once out of retries"
        );
        a.new_round(&[]);
        assert_eq!(
            a.next(7, false),
            Next::Attach,
            "a new process with the same pid starts fresh"
        );
        assert_eq!(a.done(7, false), Some(RETRY_DELAYS[0]));
    }

    #[test]
    fn success_resets_the_failures() {
        let mut a = FolderAttaches::default();
        a.next(7, false);
        a.done(7, false);
        a.next(7, false);
        a.done(7, true);
        a.next(7, false);
        assert_eq!(a.done(7, false), Some(RETRY_DELAYS[0]));
    }
}
