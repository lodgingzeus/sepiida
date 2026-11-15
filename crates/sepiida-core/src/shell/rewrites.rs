//! When to write a look value again after Windows' own code replaced it
//! (Start sizes its pinned list from code each time it opens), and when
//! to stop: a value Windows sets again at once, every time, would turn
//! into a fight, so after `MAX_REWRITES` within `WINDOW` the value is left
//! to Windows until the look is turned on again.

use std::time::{Duration, Instant};

/// Re-writes allowed within `WINDOW` before giving up.
pub const MAX_REWRITES: u8 = 5;
pub const WINDOW: Duration = Duration::from_secs(2);

/// Whether the size an element holds now (`None`: no local value, or not
/// a number) differs from the one we wrote. NaN (`Auto`) equals NaN.
pub fn size_changed(ours: f64, now: Option<f64>) -> bool {
    match now {
        Some(now) => !(now == ours || (now.is_nan() && ours.is_nan())),
        None => true,
    }
}

/// Our re-writes of one value.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rewrites {
    count: u8,
    since: Option<Instant>,
    given_up: bool,
}

/// What to do about a value Windows replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Rewrite,
    /// The limit was just reached: log once, then leave it.
    GiveUp,
    /// Given up earlier.
    Leave,
}

impl Rewrites {
    pub fn decide(&mut self, now: Instant) -> Verdict {
        if self.given_up {
            return Verdict::Leave;
        }
        if self
            .since
            .is_none_or(|since| now.duration_since(since) > WINDOW)
        {
            self.count = 0;
            self.since = Some(now);
        }
        self.count += 1;
        if self.count > MAX_REWRITES {
            self.given_up = true;
            return Verdict::GiveUp;
        }
        Verdict::Rewrite
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_changes_unless_it_is_ours() {
        assert!(!size_changed(350.0, Some(350.0)));
        assert!(size_changed(350.0, Some(168.0)));
        assert!(size_changed(350.0, None));
        assert!(!size_changed(f64::NAN, Some(f64::NAN)));
        assert!(size_changed(f64::NAN, Some(0.0)));
        assert!(size_changed(0.0, Some(f64::NAN)));
    }

    #[test]
    fn rewrites_stop_after_the_limit_within_the_window() {
        let start = Instant::now();
        let mut rewrites = Rewrites::default();
        for _ in 0..MAX_REWRITES {
            assert_eq!(rewrites.decide(start), Verdict::Rewrite);
        }
        assert_eq!(rewrites.decide(start), Verdict::GiveUp);
        assert_eq!(rewrites.decide(start + WINDOW * 10), Verdict::Leave);
    }

    #[test]
    fn rewrites_spread_out_never_give_up() {
        let start = Instant::now();
        let mut rewrites = Rewrites::default();
        for i in 0..50u32 {
            let at = start + (WINDOW + Duration::from_millis(1)) * i;
            assert_eq!(rewrites.decide(at), Verdict::Rewrite, "{i}");
        }
    }
}
