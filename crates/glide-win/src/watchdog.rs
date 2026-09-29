//! Noticing a dropped mouse hook.
//!
//! Windows silently removes a low-level hook whose callback ever takes longer
//! than `LowLevelHooksTimeout` (for example while the PC is swapping), and there
//! is no notification. The symptom is Glide quietly doing nothing. The watchdog
//! looks for the cursor moving while the hook sees no mouse events at all, which
//! can only happen if the hook is gone.

/// One observation: where the cursor is and how many events the hook has seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sample {
    pub cursor: (i32, i32),
    pub events: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Watchdog {
    last: Option<Sample>,
    strikes: u32,
}

/// Strikes in a row before declaring the hook dead. One is not enough: an app
/// can move the cursor itself (`SetCursorPos`), which the hook never sees.
const STRIKES: u32 = 2;

impl Watchdog {
    /// Records a sample; returns true when the hook should be reinstalled.
    pub(crate) fn observe(&mut self, now: Sample) -> bool {
        let Some(last) = self.last.replace(now) else {
            return false;
        };
        let moved = now.cursor != last.cursor;
        let seen = now.events != last.events;
        if seen {
            self.strikes = 0;
        } else if moved {
            self.strikes += 1;
        }
        if self.strikes >= STRIKES {
            self.reset();
            return true;
        }
        false
    }

    /// Forgets history, after the hook was (re)installed or removed on purpose.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: i32, events: u64) -> Sample {
        Sample {
            cursor: (x, 0),
            events,
        }
    }

    #[test]
    fn a_working_hook_is_left_alone() {
        let mut w = Watchdog::default();
        for (i, x) in [0, 5, 10, 10, 10, 20].into_iter().enumerate() {
            assert!(!w.observe(s(x, i as u64 * 3)));
        }
    }

    #[test]
    fn an_idle_mouse_is_not_a_dead_hook() {
        let mut w = Watchdog::default();
        for _ in 0..10 {
            assert!(!w.observe(s(7, 42)));
        }
    }

    #[test]
    fn cursor_moving_unseen_twice_means_the_hook_is_gone() {
        let mut w = Watchdog::default();
        assert!(!w.observe(s(0, 100)));
        assert!(
            !w.observe(s(10, 100)),
            "one unseen move could be SetCursorPos"
        );
        assert!(w.observe(s(20, 100)));
        // After a reinstall it starts over.
        assert!(!w.observe(s(30, 100)));
    }

    #[test]
    fn a_single_programmatic_move_is_forgiven() {
        let mut w = Watchdog::default();
        assert!(!w.observe(s(0, 100)));
        assert!(!w.observe(s(500, 100))); // an app warped the cursor
        assert!(!w.observe(s(510, 130))); // then real movement, seen by the hook
        assert!(!w.observe(s(900, 130))); // another warp: strikes were reset
    }
}
