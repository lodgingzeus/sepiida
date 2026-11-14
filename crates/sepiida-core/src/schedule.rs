//! When the background agent applies a theme.
//!
//! Wallpaper changes arrive in bursts: a slideshow advance, a Wallpaper
//! Engine playlist step and a settings save can each produce several
//! notifications. The scheduler waits until the burst is over, then asks
//! for one apply. It holds no timer of its own: the caller waits for the
//! next event or until `deadline`, whichever comes first, so an idle agent
//! sleeps without waking up.

use std::time::{Duration, Instant};

use crate::shell::hosts::Host;

/// Something the agent was told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A wallpaper source or the settings changed.
    Changed,
    /// Explorer restarted (`TaskbarCreated`) at this time: the shell
    /// styling it had is gone. The agent notes it; for the schedule it is a
    /// change. The time is taken when the message arrives, since the agent
    /// may be busy (attaching) when it handles the event.
    ExplorerRestarted(Instant),
    /// A sandboxed shell host (this process id) exited with this exit code
    /// (if it could be read) at this time, taking its styling along. For
    /// the agent's crash guard; not a change by itself (later rechecks
    /// attach to the new process).
    ShellHostExited(Host, u32, Option<u32>, Instant),
    /// A window of a sandboxed shell host (this process id) appeared:
    /// Start, quick settings, the notification centre. The agent attaches
    /// if needed and re-sends the colours (Windows may have given the
    /// surface a new brush, e.g. after a light/dark switch while it was
    /// hidden). Never schedules an apply.
    ShellWindowShown(Host, u32),
    /// The user asked for an apply; skips the wait and ignores a pause.
    ApplyNow,
    Pause,
    Resume,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Debug, Clone)]
pub struct Scheduler {
    /// Quiet time after the last change before applying.
    settle: Duration,
    /// Longest delay after the first change of a burst, so a steady stream
    /// of notifications cannot postpone the apply forever.
    max_delay: Duration,
    paused: bool,
    /// Start of the current burst and the time it is due, if any.
    pending: Option<Pending>,
    /// A change arrived while paused; apply once on resume.
    missed: bool,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    first: Instant,
    due: Instant,
}

impl Scheduler {
    pub const fn new(settle: Duration, max_delay: Duration) -> Self {
        Self {
            settle,
            max_delay,
            paused: false,
            pending: None,
            missed: false,
        }
    }

    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn handle(&mut self, event: Event, now: Instant) -> Flow {
        match event {
            Event::Changed | Event::ExplorerRestarted(_) if self.paused => self.missed = true,
            Event::Changed | Event::ExplorerRestarted(_) => self.schedule(now),
            Event::ApplyNow => {
                self.pending = Some(Pending {
                    first: now,
                    due: now,
                });
            }
            Event::Pause => {
                self.paused = true;
                self.missed |= self.pending.take().is_some();
            }
            Event::Resume => {
                self.paused = false;
                if std::mem::take(&mut self.missed) {
                    self.schedule(now);
                }
            }
            Event::ShellHostExited(..) | Event::ShellWindowShown(..) => {}
            Event::Quit => return Flow::Quit,
        }
        Flow::Continue
    }

    /// When the caller should wake up even if nothing else happens.
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.map(|p| p.due)
    }

    /// True once when an apply is due. The caller applies, and calls this
    /// again after the next event or deadline.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.pending {
            Some(p) if p.due <= now => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }

    fn schedule(&mut self, now: Instant) {
        let first = self.pending.map_or(now, |p| p.first);
        let due = match self.pending {
            // Already due ("apply now", or the caller has not woken up yet):
            // a new change must not push it back.
            Some(p) if p.due <= now => p.due,
            _ => (now + self.settle).min(first + self.max_delay),
        };
        self.pending = Some(Pending { first, due });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTLE: Duration = Duration::from_millis(500);
    const MAX: Duration = Duration::from_secs(3);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn idle_scheduler_has_no_deadline() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        assert_eq!(s.deadline(), None);
        assert!(!s.take_due(t0 + ms(10_000)));
    }

    #[test]
    fn a_burst_of_changes_gives_one_apply_after_it_settles() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        s.handle(Event::Changed, t0);
        s.handle(Event::Changed, t0 + ms(100));
        s.handle(Event::Changed, t0 + ms(300));
        assert_eq!(s.deadline(), Some(t0 + ms(800)));
        assert!(!s.take_due(t0 + ms(799)));
        assert!(s.take_due(t0 + ms(800)));
        assert!(!s.take_due(t0 + ms(900)), "only one apply per burst");
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn an_explorer_restart_schedules_like_a_change_and_respects_a_pause() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        // Handled by the agent loop itself; nothing to schedule.
        s.handle(Event::ShellWindowShown(Host::ShellExperience, 42), t0);
        s.handle(Event::ShellHostExited(Host::StartMenu, 7, Some(1), t0), t0);
        assert_eq!(s.deadline(), None);
        s.handle(Event::ExplorerRestarted(t0), t0);
        assert_eq!(s.deadline(), Some(t0 + SETTLE));
        assert!(s.take_due(t0 + SETTLE));
        s.handle(Event::Pause, t0 + ms(1_000));
        s.handle(Event::ExplorerRestarted(t0), t0 + ms(1_100));
        assert_eq!(s.deadline(), None);
        s.handle(Event::Resume, t0 + ms(2_000));
        assert_eq!(s.deadline(), Some(t0 + ms(2_000) + SETTLE));
    }

    #[test]
    fn a_steady_stream_cannot_postpone_the_apply_forever() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        let mut t = t0;
        while t < t0 + ms(5_000) {
            s.handle(Event::Changed, t);
            if s.take_due(t) {
                break;
            }
            t += ms(200);
        }
        assert!(t <= t0 + MAX, "applied at {:?}", t - t0);
    }

    #[test]
    fn apply_now_is_due_immediately_and_a_later_change_keeps_it_early() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        s.handle(Event::ApplyNow, t0);
        s.handle(Event::Changed, t0 + ms(1));
        assert_eq!(s.deadline(), Some(t0));
        assert!(s.take_due(t0 + ms(1)));
    }

    #[test]
    fn apply_now_works_while_paused() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        s.handle(Event::Pause, t0);
        s.handle(Event::ApplyNow, t0);
        assert!(s.take_due(t0));
        assert!(s.is_paused());
    }

    #[test]
    fn changes_while_paused_apply_once_on_resume() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        s.handle(Event::Pause, t0);
        s.handle(Event::Changed, t0 + ms(10));
        s.handle(Event::Changed, t0 + ms(20));
        assert_eq!(s.deadline(), None);
        assert!(!s.take_due(t0 + ms(5_000)));

        s.handle(Event::Resume, t0 + ms(6_000));
        assert_eq!(s.deadline(), Some(t0 + ms(6_500)));
        assert!(s.take_due(t0 + ms(6_500)));

        s.handle(Event::Resume, t0 + ms(7_000));
        assert_eq!(s.deadline(), None, "nothing was missed the second time");
    }

    #[test]
    fn pausing_with_a_pending_change_defers_it_to_resume() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        s.handle(Event::Changed, t0);
        s.handle(Event::Pause, t0 + ms(100));
        assert!(!s.take_due(t0 + ms(1_000)));
        s.handle(Event::Resume, t0 + ms(2_000));
        assert!(s.take_due(t0 + ms(2_500)));
    }

    #[test]
    fn quit_stops_the_loop() {
        let mut s = Scheduler::new(SETTLE, MAX);
        let t0 = Instant::now();
        assert_eq!(s.handle(Event::Changed, t0), Flow::Continue);
        assert_eq!(s.handle(Event::Quit, t0), Flow::Quit);
    }
}
